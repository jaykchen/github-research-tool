pub mod data_analyzers;
pub mod github_data_fetchers;
pub mod utils;
use data_analyzers::*;
use discord_flows::{
    http::HttpBuilder,
    model::{application_command::CommandDataOptionValue, guild},
    Bot, EventModel, ProvidedBot,
};
use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use github_data_fetchers::*;
use http_req::{
    request::{Method, Request},
    uri::Uri,
};
use serde::Deserialize;
use serde_json;
use slack_flows::send_message_to_channel;
use std::env;
use utils::*;

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn run() {
    dotenv().ok();
    logger::init();
    let discord_token = env::var("discord_token").unwrap();
    let commands_registered = env::var("COMMANDS_REGISTERED").unwrap_or("false".to_string());

    match commands_registered.as_str() {
        "false" => {
            register_commands(&discord_token).await;
            env::set_var("COMMANDS_REGISTERED", "true");
        }
        _ => {}
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
                "name": "type",
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
    //     // Define the Discord API endpoint for registering commands
    //     let uri = format!(
    //         "https://discord.com/api/v8/applications/{}/guilds/{}/commands",
    //         bot_id, guild_id
    //     );
    // let commands = vec![command_get_user_repos, command_search_mention, command_save_user];
    let http_client = HttpBuilder::new(discord_token)
        .application_id(bot_id.parse().unwrap())
        .build();

    match http_client
        .create_guild_application_command(guild_id, &command_get_user_repos)
        .await
    {
        Ok(_) => log::info!("Successfully registered command"),
        Err(err) => log::error!("Error registering command: {}", err),
    }
}
async fn handle<B: Bot>(bot: &B, em: EventModel) {
    match em {
        EventModel::ApplicationCommand(ac) => {
            let client = bot.get_client();
            let channel_id = ac.channel_id.as_u64();

            let initial_response = serde_json::json!(
                {
                    "type": 5,  // type 5 indicates that the bot has received the command and is processing it
                }
            );
            _ = client
                .create_interaction_response(ac.id.0, &ac.token, &initial_response)
                .await;

            match ac.data.name.as_str() {
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

                    let user_repos = get_user_repos(username, language).await;

                    let resp = serde_json::json!({
                        "type": 4, // type 4 is for Channel Message With Source
                        "data": {
                            "content": user_repos
                        }
                    });
                    _ = client.send_message(*channel_id, &resp).await;
                    _ = client.create_followup_message(&ac.token, &resp).await;
                }
                _ => {
                    let default_resp = serde_json::json!({
                        "type": 4,
                        "data": {
                            "content": "Unknown command."
                        }
                    });
                    _ = client
                        .create_followup_message(&ac.token, &default_resp)
                        .await;
                }
            }
        }
        EventModel::Message(msg) => {
            let client = bot.get_client();
            let channel_id = msg.channel_id;
            let content = msg.content;
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
