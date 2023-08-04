pub mod data_analyzers;
pub mod github_data_fetchers;
pub mod utils;
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
    // let commands_registered = env::var("COMMANDS_REGISTERED").unwrap_or("false".to_string());

    if !commands_registered {
        register_commands(&discord_token).await;
        commands_registered = true;
    }

    let bot = ProvidedBot::new(discord_token);
    bot.listen(|em| handle(&bot, em)).await;
}

async fn register_commands(discord_token: &str) {
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
        Ok(_) => log::info!("Successfully registered command"),
        Err(err) => log::error!("Error registering command: {}", err),
    }
}
async fn handle<B: Bot>(bot: &B, em: EventModel) {
    let mut resp = serde_json::json!({"type": 4, "data": {
        "content": "not getting anything"
    }});
    let client = bot.get_client();
    // let channel_id = ac.channel_id.as_u64();
    // let channel_id = env::var("discord_channel_id").unwrap_or("1128056246570860617".to_string());
    // let application_id = env::var("application_id").unwrap_or("1132483335906664599".to_string());
    // let channel_id = channel_id.parse::<u64>().unwrap_or(1128056246570860617);
    // let application_id = application_id.parse::<u64>().unwrap_or(1132483335906664599);
    // let mut application_id: InteractionId = InteractionId(0);
    // let mut interaction_token = String::from("");
    match em {
        EventModel::ApplicationCommand(ac) => {
            // application_id = ac.id;
            // interaction_token = ac.token.clone();
            // client.set_application_id(1132483335906664599);
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

                    let search_result = search_issue(search_query)
                        .await
                        .unwrap_or("Couldn't find anything!".to_string());
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

// async fn handler(workspace: &str, channel: &str, sm: SlackMessage) {
//     let trigger_word = env::var("trigger_word").unwrap_or("bot@get".to_string());

//     let parts: Vec<&str> = sm
//         .text
//         .split(&trigger_word)
//         .nth(1) // skip the part before "bot@get"
//         .unwrap_or("") // if "bot@get" is not found, use an empty string
//         .split_whitespace()
//         .collect();

//     let (owner, repo, user_name, language) = match parts.as_slice() {
//         [owner, repo, user, language, ..] => (owner, repo, user, language),
//         _ => panic!("Input should contain 'bot@get <github_owner> <github_repo> <user_name>'"),
//     };

//     log::info!("language: {:?}", language.to_string());
//     if sm.text.contains(&trigger_word) {
//         if let Some(res) = get_user_repos(user_name, language).await {
//             send_message_to_channel("ik8", "ch_in", res).await;
//         }

//         let search_query = user_name.to_string();
//         if let Some(res) = search_mention(&search_query, Some("ISSUE")).await {
//             send_message_to_channel("ik8", "ch_in", res).await;
//         }
//         if let Some(res) = search_mention(&search_query, Some("Repository")).await {
//             send_message_to_channel("ik8", "ch_mid", res).await;
//         }
//         if let Some(res) = search_mention(&search_query, Some("PULL_REQUEST")).await {
//             send_message_to_channel("ik8", "ch_out", res).await;
//         }
//         if let Some(res) = search_mention(&search_query, Some("DISCUSSION")).await {
//             send_message_to_channel("ik8", "ch_err", res).await;
//         }

//         if !save_user(user_name).await {
//             send_message_to_channel("ik8", "ch_mid", format!("{user_name} is a new contributor"))
//                 .await;
//         }
//     }
// }
