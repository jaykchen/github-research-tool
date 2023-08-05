pub mod data_analyzers;
pub mod github_data_fetchers;
pub mod utils;
pub mod octocrab_compat;
use data_analyzers::*;
use discord_flows::{
    http::HttpBuilder,
    model::{
        application_command::CommandDataOptionValue, channel, guild, interaction, Interaction,
    },
    Bot, EventModel, ProvidedBot,
};
use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use github_data_fetchers::*;
use http_req::{
    request::{Method, Request},
    response,
    uri::Uri,
};
use serde::Deserialize;
use serde_json;
use slack_flows::send_message_to_channel;
use std::env;
use store_flows::{get, set};
use utils::*;

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn run() {
    dotenv().ok();
    logger::init();
    let discord_token = env::var("discord_token").unwrap();
    let mut commands_registered = false;

    match get("gh_bot_commands_registered") {
        Some(s) => {
            commands_registered = serde_json::from_value::<bool>(s).unwrap_or_else(|_| false);
        }
        None => {}
    }

    if !commands_registered {
        match register_commands(&discord_token).await {
            false => set("gh_bot_commands_registered", serde_json::json!(false), None),
            true => set("gh_bot_commands_registered", serde_json::json!(true), None),
        };
    }

    let bot = ProvidedBot::new(discord_token);
    bot.listen(|em| handle(&bot, em)).await;
}

async fn register_commands(discord_token: &str) -> bool {
    let command_get_user_repos = serde_json::json!({
        "name": "get_user_repos",
        "description": "Get user's top repos by programming lanugage",
        "options": [
            {
                "name": "username",
                "description": "The username to lookup",
                "type": 3,
                "required": true
            },
            {
                "name": "language",
                "description": "The repo's programming language",
                "type": 3,
                "required": true
            }
        ]
    });

    let command_search_mention = serde_json::json!({
        "name": "search_mention",
        "description": "Search for mentions in issues",
        "options": [
            {
                "name": "search_query",
                "description": "The query to search mentions for",
                "type": 3, // String type according to Discord's API
                "required": true
            },
            {
                "name": "search_type",
                "description": "The type to search mentions in (e.g., ISSUE)",
                "type": 3, // String type according to Discord's API
                "required": true
            }
        ]
    });
    let command_save_user = serde_json::json!({
        "name": "save_user",
        "description": "Check whether a username already exists, save it if new",
        "options": [
            {
                "name": "username",
                "description": "The username to save",
                "type": 3, // String type according to Discord's API
                "required": true
            }
        ]
    });

    let bot_id = env::var("bot_id").unwrap_or("1124137839601406013".to_string());
    // let channel_id = env::var("discord_channel_id").unwrap_or("1128056246570860617".to_string());
    let guild_id = env::var("discord_guild_id").unwrap_or("1128056245765558364".to_string());
    let guild_id = guild_id.parse::<u64>().unwrap_or(1128056245765558364);
    let commands = serde_json::json!([
        command_get_user_repos,
        command_save_user,
        command_search_mention,
    ]);
    let http_client = HttpBuilder::new(discord_token)
        .application_id(bot_id.parse().unwrap())
        .build();

    match http_client
        .create_guild_application_commands(guild_id, &commands)
        .await
    {
        Ok(_) => {
            log::info!("Successfully registered command");
            true
        }
        Err(err) => {
            log::error!("Error registering command: {}", err);
            false
        }
    }
}
async fn handle<B: Bot>(bot: &B, em: EventModel) {
    let mut resp = serde_json::json!({"type": 4, "data": {
        "content": "not getting anything"
    }});
    let client = bot.get_client();

    match em {
        EventModel::ApplicationCommand(ac) => {
            let initial_response = serde_json::json!(
                    {
            "type": 4,
            "data": {
                "content": "Bot is pulling data for you, please wait."
            }
            }
                );
            _ = client
                .create_interaction_response(ac.id.into(), &ac.token, &initial_response)
                .await;
            client.set_application_id(ac.application_id.into());
            match ac.data.name.as_str() {
                "save_user" => {
                    let options = ac
                        .data
                        .options
                        .get(0)
                        .expect("Expected username")
                        .resolved
                        .as_ref()
                        .expect("Expected username object");

                    let username = match options {
                        CommandDataOptionValue::String(s) => s,
                        _ => panic!("Expected string for username"),
                    };
                    save_user(username).await;

                    let usernames = get("usernames")
                        .unwrap_or(serde_json::json!({}))
                        .to_string();
                    send_message_to_channel("ik8", "ch_in", usernames.to_string()).await;

                    resp = serde_json::json!({
                        "content": usernames
                    });
                    match client
                        .edit_original_interaction_response(&ac.token, &resp)
                        .await
                    {
                        Ok(_) => {}
                        Err(_e) => log::error!("error sending save_user message: {:?}", _e),
                    }
                }
                "get_user_repos" => {
                    let options = &ac.data.options;

                    let username = match options
                        .get(0)
                        .expect("Expected username option")
                        .resolved
                        .as_ref()
                        .expect("Expected username object")
                    {
                        CommandDataOptionValue::String(s) => s,
                        _ => panic!("Expected string for username"),
                    };

                    let language = match options
                        .get(1)
                        .expect("Expected language option")
                        .resolved
                        .as_ref()
                        .expect("Expected language object")
                    {
                        CommandDataOptionValue::String(s) => s,
                        _ => panic!("Expected string for language"),
                    };

                    let user_repos = get_user_repos(username, language)
                        .await
                        .unwrap_or("Couldn't get any repos!".to_string());

                    resp = serde_json::json!({
                        "content": user_repos.to_string()
                    });
                    send_message_to_channel("ik8", "ch_in", user_repos.to_string()).await;

                    match client
                        .edit_original_interaction_response(&ac.token, &resp)
                        .await
                    {
                        Ok(_) => {}
                        Err(_e) => log::error!("error sending get_user_repos message: {:?}", _e),
                    }
                }
                "search_mention" => {
                    let options = &ac.data.options;

                    let search_query = match options
                        .get(0)
                        .expect("Expected search_query option")
                        .resolved
                        .as_ref()
                        .expect("Expected search_query object")
                    {
                        CommandDataOptionValue::String(s) => s,
                        _ => panic!("Expected string for search_query"),
                    };

                    let search_type = match options
                        .get(1)
                        .expect("Expected search_type option")
                        .resolved
                        .as_ref()
                        .expect("Expected search_type object")
                    {
                        CommandDataOptionValue::String(s) => s,
                        _ => panic!("Expected string for search_type"),
                    };

                    let mut search_result = "".to_string();
                    match search_type.to_lowercase().as_str() {
                        "issue" => {
                            search_result =
                                search_issue(search_query).await.unwrap_or("".to_string())
                        }
                        "repository" => {
                            search_result =
                                search_issue(search_query).await.unwrap_or("".to_string())
                        }
                        "discussion" => {
                            search_result =
                                search_issue(search_query).await.unwrap_or("".to_string())
                        }
                        _ => unreachable!("invalid search_type"),
                    }

                    send_message_to_channel("ik8", "ch_in", search_result.to_string()).await;

                    resp = serde_json::json!({
                        "content": search_result.to_string()
                    });
                    match client
                        .edit_original_interaction_response(&ac.token, &resp)
                        .await
                    {
                        Ok(_) => {}
                        Err(_e) => log::error!("error sending search_mention message: {:?}", _e),
                    }
                }
                _ => {}
            }
        }
        EventModel::Message(msg) => {
            resp = serde_json::json!({"type": 4, "data": {
                "content": msg.content
            }});
        }
    }
}
