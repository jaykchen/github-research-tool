use discord_flows::http::Http;
use discord_flows::http::HttpBuilder;
use serde_json;
use std::env;

pub async fn register_commands(discord_token: &str) -> bool {
    let bot_id = env::var("bot_id").unwrap_or("1124137839601406013".to_string());
    let guild_id = env::var("discord_server").unwrap_or("1128056245765558364".to_string());

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

    let guild_id = guild_id.parse::<u64>().unwrap_or(1128056245765558364);
    let commands = serde_json::json!([command_weekly_report,]);
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

pub async fn edit_original_wrapped(
    client: &Http,
    token: &str,
    content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match client
        .edit_original_interaction_response(token, &serde_json::json!({ "content": content }))
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            log::error!("error sending message: {:?}", e);
            Err(Box::new(e))
        }
    }
}
