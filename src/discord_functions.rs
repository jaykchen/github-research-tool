use discord_flows::http::HttpBuilder;
use serde_json;
use std::env;

pub async fn register_once(discord_token: &str) {
    let mut registered = false;

    let discord_token = env::var("discord_token").unwrap();
    if !registered {
        register_commands(&discord_token).await;
    }
}

pub async fn register_commands(discord_token: &str) -> bool {
    let command_weekly_report = serde_json::json!({
        "name": "weekly_report",
        "description": "Generate a weekly report",
        "options": [
            {
                "name": "owner",
                "description": "The owner of the repository",
                "type": 3, // type 3 indicates a STRING
                "required": true
            },
            {
                "name": "repo",
                "description": "The repository name",
                "type": 3,
                "required": true
            },
            {
                "name": "user_name",
                "description": "The username for report generation",
                "type": 3,
                "required": false
            }

        ]
    });

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

    let command_search = serde_json::json!({
        "name": "search",
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

    let bot_id = env::var("bot_id").unwrap_or("1124137839601406013".to_string());
    // let channel_id = env::var("discord_channel_id").unwrap_or("1128056246570860617".to_string());
    let guild_id = env::var("discord_guild_id").unwrap_or("1128056245765558364".to_string());
    let guild_id = guild_id.parse::<u64>().unwrap_or(1128056245765558364);
    let commands = serde_json::json!([
        command_weekly_report,
        command_get_user_repos,
        command_search,
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
