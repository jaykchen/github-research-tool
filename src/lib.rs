pub mod data_analyzers;
pub mod discord_functions;
pub mod github_data_fetchers;
pub mod octocrab_compat;
pub mod reports;
pub mod utils;
use chrono::{ Duration, Utc };
use data_analyzers::*;
use discord_flows::{
    model::application::interaction::application_command::ApplicationCommandInteraction,
    http::Http,
    model::{ application_command::CommandDataOptionValue },
    // model::{ application_command::CommandDataOptionValue, channel, guild, Interaction },
    Bot,
    EventModel,
    ProvidedBot,
};
use discord_functions::*;
use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use github_data_fetchers::*;
use serde_json::Value;
use slack_flows::send_message_to_channel;
use std::env;
use tokio::time::{ sleep };

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn run() {
    dotenv().ok();
    logger::init();
    let discord_token = env::var("discord_token").unwrap();

    // register discord slash command, only need to run once after deployment
    // the author hasn't figured out how to achieve the run once in programs' lifetime,
    // you need to disable this line after first successful run
    // compile the program again and run it again.
    // let _ = register_commands(&discord_token).await;

    let bot = ProvidedBot::new(discord_token);
    bot.listen(|em| handle(&bot, em)).await;
}

async fn handle<B: Bot>(bot: &B, em: EventModel) {
    let client = bot.get_client();
    let mut report = String::new();
    match em {
        EventModel::ApplicationCommand(ac) => {
            let initial_response =
                serde_json::json!(
                {
                    "type": 4,
                    "data": {
                        "content": "🤖 ready."
                    }
                }
            );
            _ = client.create_interaction_response(
                ac.id.into(),
                &ac.token,
                &initial_response
            ).await;
            client.set_application_id(ac.application_id.into());

            match ac.data.name.as_str() {
                "weekly_report" => {
                    handle_weekly_report(bot, &client, ac).await;
                }
                "get_user_repos" => {
                    // handle_get_user_repos(bot, &client, ac).await;
                }
                "search" => {
                    // handle_search(bot, &client, ac).await;
                }
                _ => {}
            }
        }
        EventModel::Message(_) => {
            // keep it empty for now
        }
    }
}

async fn handle_weekly_report<B: Bot>(_bot: &B, client: &Http, ac: ApplicationCommandInteraction) {
    let options = &ac.data.options;
    let n_days = 7u16;
    let mut report = String::new();
    let owner = match
        options
            .get(0)
            .expect("Expected owner option")
            .resolved.as_ref()
            .expect("Expected owner object")
    {
        CommandDataOptionValue::String(s) => s,
        _ => panic!("Expected string for owner"),
    };

    let repo = match
        options
            .get(1)
            .expect("Expected repo option")
            .resolved.as_ref()
            .expect("Expected repo object")
    {
        CommandDataOptionValue::String(s) => s,
        _ => panic!("Expected string for repo"),
    };

    let mut _profile_data = None;
    match is_valid_owner_repo(owner, repo).await {
        None => {
            sleep(tokio::time::Duration::from_secs(1)).await;
            let _ = send_discord_msg(
                client,
                &ac.token,
                "You've entered invalid owner/repo, or the target is private. Please try again."
            ).await;
            return;
        }
        Some(gm) => {
            _profile_data = Some(gm.payload);
        }
    }

    let user_name = options.get(2).and_then(|opt| {
        opt.resolved.as_ref().and_then(|val| {
            match val {
                CommandDataOptionValue::String(s) => Some(s.as_str()),
                _ => None,
            }
        })
    });

    match user_name {
        Some(user_name) => {
            if !is_code_contributor(owner, repo, &user_name).await {
                sleep(tokio::time::Duration::from_secs(1)).await;
                let content = format!(
                    "{user_name} hasn't contributed code to {owner}/{repo}. Bot will try to find out {user_name}'s other contributions."
                );
                let _ = send_discord_msg(client, &ac.token, &content).await;
            }
        }
        None => {
            let content = format!(
                "You didn't input a user's name. Bot will then create a report on the weekly progress of {owner}/{repo}."
            );
            let _ = send_discord_msg(client, &ac.token, &content).await;
        }
    }

    let addressee_str = user_name.map_or(String::from("key community participants'"), |n|
        format!("{n}'s")
    );

    let start_msg_str = format!(
        "exploring {addressee_str} GitHub contributions to `{owner}/{repo}` project"
    );
    sleep(tokio::time::Duration::from_secs(1)).await;

    let _ = send_discord_msg(client, &ac.token, &start_msg_str).await;

    let mut commits_summaries = String::new();

    match get_commits_in_range(&owner, &repo, user_name, n_days).await {
        Some((count, commits_vec)) => {
            let commits_str = commits_vec
                .iter()
                .map(|com| {
                    com.source_url
                        .rsplitn(2, '/')
                        .nth(0)
                        .unwrap_or("1234567")
                        .chars()
                        .take(7)
                        .collect::<String>()
                })
                .collect::<Vec<String>>()
                .join(", ");
            let commits_msg_str = format!("found {count} commits: {commits_str}");
            report.push_str(commits_msg_str.as_str());
            let _ = send_discord_msg(client, &ac.token, &commits_msg_str).await;

            if let Some((a, _, commit_vec)) = process_commits(commits_vec).await {
                let text = commit_vec
                    .into_iter()
                    .map(|commit| format!("\n{}: {}\n", commit.source_url, commit.tag_line))
                    .collect::<Vec<String>>()
                    .join("");
                send_message_to_channel("ik8", "ch_rep", text).await;
                commits_summaries = a;
            }
        }
        None => log::error!("failed to get commits"),
    }
    let mut issues_summaries = String::new();

    match get_issues_in_range(&owner, &repo, user_name, n_days).await {
        Some((count, issue_vec)) => {
            let issues_str = issue_vec
                .iter()
                .map(|issue| issue.url.rsplitn(2, '/').nth(0).unwrap_or("1234"))
                .collect::<Vec<&str>>()
                .join(", ");
            let issues_msg_str = format!("found {} issues: {}", count, issues_str);
            report.push_str(issues_msg_str.as_str());
            let _ = send_discord_msg(client, &ac.token, &issues_msg_str).await;

            if let Some((summary, _, issues_vec)) = process_issues(issue_vec, user_name).await {
                let text = issues_vec
                    .into_iter()
                    .map(|commit| format!("\n{}: {}\n", commit.source_url, commit.tag_line))
                    .collect::<Vec<String>>()
                    .join("");
                send_message_to_channel("ik8", "ch_iss", text).await;

                issues_summaries = summary;
            };
        }
        None => log::error!("failed to get issues"),
    }

    let now = Utc::now();
    let a_week_ago = now - Duration::days(n_days as i64);
    let a_week_ago_str = a_week_ago.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let discussion_query = format!("involves:{} updated:>{}", user_name.unwrap(), a_week_ago_str);

    let mut discussion_data = String::new();
    match search_discussions(&discussion_query).await {
        Some((count, discussion_vec)) => {
            let discussions_str = discussion_vec
                .iter()
                .map(|discussion| {
                    discussion.source_url.rsplitn(2, '/').nth(0).unwrap_or("1234")
                })
                .collect::<Vec<&str>>()
                .join(", ");
            let discussions_msg_str = format!("found {} discussions: {}", count, discussions_str);
            report.push_str(discussions_msg_str.as_str());
            let _ = send_discord_msg(client, &ac.token, &discussions_msg_str).await;

            let (a,discussions_vec) = analyze_discussions(discussion_vec, user_name).await;

            let text = discussions_vec
                .into_iter()
                .map(|dis| format!("\n{}: {}\n", dis.source_url, dis.tag_line))
                .collect::<Vec<String>>()
                .join("");
            send_message_to_channel("ik8", "ch_dis", text).await;
            discussion_data = a;


        }
        None => log::error!("failed to get discussions"),
    }

    let resp_content = correlate_commits_issues_discussions(
        Some(&commits_summaries),
        Some(&issues_summaries),
        Some(&discussion_data),
        user_name
    ).await.unwrap_or("Failed to generate report.".to_string());
    report.push_str(resp_content.as_str());
    let _ = send_discord_msg(client, &ac.token, &report).await;
}

async fn send_discord_msg(
    client: &Http,
    token: &str,
    content: &str
) -> Result<(), Box<dyn std::error::Error>> {
    match
        client.edit_original_interaction_response(
            token,
            &serde_json::json!({ "content": content })
        ).await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            log::error!("error sending message: {:?}", e);
            Err(Box::new(e))
        }
    }
}

async fn handle_search<B: Bot>(bot: &B, client: &Http, ac: ApplicationCommandInteraction) {
    let options = &ac.data.options;

    let search_query = match
        options
            .get(0)
            .expect("Expected search_query option")
            .resolved.as_ref()
            .expect("Expected search_query object")
    {
        CommandDataOptionValue::String(s) => s,
        _ => panic!("Expected string for search_query"),
    };

    let search_type = match
        options
            .get(1)
            .expect("Expected search_type option")
            .resolved.as_ref()
            .expect("Expected search_type object")
    {
        CommandDataOptionValue::String(s) => s,
        _ => panic!("Expected string for search_type"),
    };

    let mut search_result = "".to_string();
    match search_type.to_lowercase().as_str() {
        "issue" => {
            search_result = search_issue(&search_query).await.unwrap_or("".to_string());
        }
        "users" => {
            search_result = search_users(&search_query).await.unwrap_or("".to_string());
        }
        "repository" => {
            search_result = search_repository(&search_query).await.unwrap_or("".to_string());
        }
        "discussion" => {
            // Add the logic for discussion here, if required
        }
        _ => unreachable!("invalid search_type"),
    }

    let search_result = search_result.chars().take(500).collect::<String>();
    let resp = serde_json::json!({
        "content": search_result.to_string()
    });
    match client.edit_original_interaction_response(&ac.token, &resp).await {
        Ok(_) => {}
        Err(_e) => log::error!("error sending search message: {:?}", _e),
    }
}

async fn handle_get_user_repos<B: Bot>(bot: &B, client: &Http, ac: ApplicationCommandInteraction) {
    let options = &ac.data.options;

    let username = match
        options
            .get(0)
            .expect("Expected username option")
            .resolved.as_ref()
            .expect("Expected username object")
    {
        CommandDataOptionValue::String(s) => s,
        _ => panic!("Expected string for username"),
    };

    let language = match
        options
            .get(1)
            .expect("Expected language option")
            .resolved.as_ref()
            .expect("Expected language object")
    {
        CommandDataOptionValue::String(s) => s,
        _ => panic!("Expected string for language"),
    };

    let user_repos = get_user_repos_gql(&username, &language).await.unwrap_or(
        "Couldn't get any repos!".to_string()
    );

    let resp = serde_json::json!({
        "content": user_repos.to_string()
    });

    match client.edit_original_interaction_response(&ac.token, &resp).await {
        Ok(_) => {}
        Err(_e) => log::error!("error sending get_user_repos message: {:?}", _e),
    }
}
