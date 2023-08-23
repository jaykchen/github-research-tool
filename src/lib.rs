pub mod data_analyzers;
pub mod discord_functions;
pub mod github_data_fetchers;
pub mod octocrab_compat;
pub mod reports;
pub mod utils;
use chrono::{Duration, Utc};
use data_analyzers::*;
use discord_flows::{
    application_command_handler, message_handler,
    http::Http,
    model::{
        // application::interaction::InteractionResponseType,
        application_command::CommandDataOptionValue,
        prelude::application::interaction::application_command::ApplicationCommandInteraction,
        Message,
    },
    Bot, ProvidedBot,
};
use discord_functions::*;
use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use github_data_fetchers::*;
use serde_json::json;
use std::{env, vec};
use tokio::time::sleep;

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn on_deploy() {
    dotenv().ok();
    logger::init();
    let discord_token = env::var("discord_token").unwrap();
    let channel_id = env::var("discord_channel_id").unwrap_or("channel_id not found".to_string());
    let bot = ProvidedBot::new(&discord_token);
    let commands_registered = env::var("COMMANDS_REGISTERED").unwrap_or("false".to_string());

    match commands_registered.as_str() {
        "false" => {
            register_commands(&discord_token).await;
            env::set_var("COMMANDS_REGISTERED", "true");
        }
        _ => {}
    }

    bot.listen_to_messages().await;

    let channel_id = channel_id.parse::<u64>().unwrap();
    bot.listen_to_application_commands_from_channel(channel_id)
        .await;
}

#[message_handler]
async fn handle(msg: Message) {
    let discord_token = env::var("discord_token").unwrap();
    let bot = ProvidedBot::new(&discord_token);
    let client = bot.get_client();

    if msg.author.bot {
        std::process::exit(0);
    }

    _ = client
        .send_message(
            msg.channel_id.into(),
            &json!({
                "content": msg.content,
            }),
        )
        .await;

}

#[application_command_handler]
async fn handler(ac: ApplicationCommandInteraction) {
    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let token = env::var("discord_token").unwrap();
    let _bot = ProvidedBot::new(&token);
    let client = _bot.get_client();
    client.set_application_id(ac.application_id.into());

    let options = &ac.data.options;
    _ = client
        .create_interaction_response(
            ac.id.into(),
            &ac.token,
            &(json!(
                {
                    "type": 4,
                    "data": {
                        "content": "🤖 ready."
                    }
                }
            )),
        )
        .await;

    match ac.data.name.as_str() {
        "weekly_report" => _= handle_weekly_report(client, ac, github_token).await,

        "search" => {
            // handle_search(bot, &client, ac).await;
        }
        _ => {}
    }
}

async fn handle_weekly_report(
    client: Http,
    ac: ApplicationCommandInteraction,
    github_token: String,
) {
    let options = &ac.data.options;
    let n_days = 7u16;
    let mut report = Vec::<String>::new();
    let owner = match options
        .get(0)
        .expect("Expected owner option")
        .resolved
        .as_ref()
        .expect("Expected owner object")
    {
        CommandDataOptionValue::String(s) => s,
        _ => panic!("Expected string for owner"),
    };
    let repo = match options
        .get(1)
        .expect("Expected repo option")
        .resolved
        .as_ref()
        .expect("Expected repo object")
    {
        CommandDataOptionValue::String(s) => s,
        _ => panic!("Expected string for repo"),
    };
    let mut _profile_data = String::new();
    match is_valid_owner_repo_integrated(&github_token, owner, repo).await {
        None => {
            _ = client
                .edit_original_interaction_response(&ac.token, &(json!({ "content": "You've entered invalid owner/repo, or the target is private. Please try again." })))
                .await;

            std::process::exit(0);
        }
        Some(gm) => {
            _profile_data = format!("About {}/{}: {}", owner, repo, gm.payload);
        }
    }
    let user_name = options.get(2).and_then(|opt| {
        opt.resolved.as_ref().and_then(|val| match val {
            CommandDataOptionValue::String(s) => Some(s.to_string()),
            _ => None,
        })
    });
    let mut msg_content = String::new();
    let mut addressee_str = String::from("key community participants'");
    let mut report_placeholder = vec!["No useful data found, nothing to report".to_string()];
    'user_name_check_block: {
        match &user_name {
                        Some(user_name) => {
                            addressee_str = format!("{user_name}'s");
                            report_placeholder=    vec![format!("No useful data found for {user_name}, you may try `/search` to find out more about {user_name}" )];     
                                       if is_code_contributor(&github_token, owner, repo, &user_name).await {
                                break 'user_name_check_block;
                            };
                            msg_content = format!("{user_name} hasn't contributed code to {owner}/{repo}. Bot will try to find out {user_name}'s other contributions.");

                        }
                        None => msg_content = format!(
                            "You didn't input a user's name. Bot will then create a report on the weekly progress of {owner}/{repo}."
                        ),
                    };
    }
    if !msg_content.is_empty() {
        _ = client
            .edit_original_interaction_response(&ac.token, &(json!({"content": msg_content})))
            .await;
    }
    msg_content =
        format!("exploring {addressee_str} GitHub contributions to `{owner}/{repo}` project");
    sleep(tokio::time::Duration::from_secs(2)).await;
    _ = client
        .edit_original_interaction_response(&ac.token, &(json!({"content": msg_content})))
        .await;

    let mut commits_summaries = String::new();
    'commits_block: {
        match get_commits_in_range(&github_token, &owner, &repo, user_name.clone(), n_days).await {
            Some((count, mut commits_vec)) => {
                let commits_str = commits_vec
                    .iter()
                    .map(|com| com.source_url.to_owned())
                    .collect::<Vec<String>>()
                    .join("\n");

                msg_content = format!("found {count} commits:\n{commits_str}");

                report.push(msg_content.clone());
                _ = client
                    .edit_original_interaction_response(
                        &ac.token,
                        &(json!({"content": msg_content})),
                    )
                    .await;

                if count == 0 {
                    break 'commits_block;
                }
                match process_commits(&github_token, &mut commits_vec).await {
                    Some(summary) => {
                        commits_summaries = summary;
                    }
                    None => log::error!("processing commits failed"),
                }
            }
            None => log::error!("failed to get commits"),
        }
    }

    let mut issues_summaries = String::new();
    'issues_block: {
        match get_issues_in_range(&github_token, &owner, &repo, user_name.clone(), n_days).await {
            Some((count, issue_vec)) => {
                let issues_str = issue_vec
                    .iter()
                    .map(|issue| issue.html_url.to_owned())
                    .collect::<Vec<String>>()
                    .join("\n");

                msg_content = format!("found {count} issues:\n{issues_str}");

                report.push(msg_content.clone());
                _ = client
                    .edit_original_interaction_response(
                        &ac.token,
                        &(json!({"content": msg_content})),
                    )
                    .await;

                if count == 0 {
                    break 'issues_block;
                }

                match process_issues(&github_token, issue_vec, user_name.clone()).await {
                    Some((summary, _, issues_vec)) => {
                        issues_summaries = summary;
                    }
                    None => log::error!("processing issues failed"),
                }
            }
            None => log::error!("failed to get issues"),
        }
    }

    let n_plus_30_days_ago_str = (Utc::now() - Duration::days(n_days as i64 + 30))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let discussion_query = match &user_name {
        Some(user_name) => {
            format!("repo:{owner}/{repo} involves: {user_name} updated:>{n_plus_30_days_ago_str}")
        }
        None => format!("repo:{owner}/{repo} updated:>{n_plus_30_days_ago_str}"),
    };
    let mut discussion_data = String::new();
    match search_discussions_integrated(&github_token, &discussion_query, &user_name).await {
        Some((summary, discussion_vec)) => {
            let count = discussion_vec.len();
            let discussions_str = discussion_vec
                .iter()
                .map(|discussion| discussion.source_url.to_owned())
                .collect::<Vec<String>>()
                .join("\n");

            msg_content =
                format!("{count} discussions were referenced in analysis:\n {discussions_str}");
            report.push(msg_content.clone());

            discussion_data = summary;
        }
        None => log::error!("failed to get discussions"),
    }
    _ = client
        .edit_original_interaction_response(&ac.token, &(json!({"content": msg_content})))
        .await;

    if commits_summaries.is_empty() && issues_summaries.is_empty() && discussion_data.is_empty() {
        report = report_placeholder;
    } else {
        match correlate_commits_issues_discussions(
            Some(&_profile_data),
            Some(&commits_summaries),
            Some(&issues_summaries),
            Some(&discussion_data),
            user_name.as_deref(),
        )
        .await
        {
            None => {
                report = vec!["no report generated".to_string()];
            }
            Some(final_summary) => {
                report.push(final_summary);
            }
        }
    }
    _ = client
        .edit_original_interaction_response(&ac.token, &(json!({"content": report.join("\n")})))
        .await;
}
