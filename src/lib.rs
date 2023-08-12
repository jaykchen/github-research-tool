pub mod data_analyzers;
pub mod discord_functions;
pub mod github_data_fetchers;
pub mod octocrab_compat;
pub mod reports;
pub mod utils;
use chrono::{ Duration, Utc };
use data_analyzers::*;
use discord_flows::model::interactions::application_command::{
    ApplicationCommand,
    ApplicationCommandInteraction,
};
use discord_flows::{
    http::Http,
    model::{ application_command::CommandDataOptionValue, channel, guild, Interaction },
    Bot,
    EventModel,
    ProvidedBot,
};
use discord_functions::*;
use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use github_data_fetchers::*;
use reports::*;
use serde_json;
use slack_flows::send_message_to_channel;
use std::env;

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

async fn handle_weekly_report<B: Bot>(bot: &B, client: &Http, ac: ApplicationCommandInteraction) {
    let options = &ac.data.options;
    let mut n_days = 7u16;
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

    let mut profile_data = None;
    match is_valid_owner_repo(owner, repo).await {
        None => {
            match
                client.edit_original_interaction_response(
                    &ac.token,
                    &serde_json::json!({
                                "content": "You've entered invalid owner/repo, or the target is private. Please try again."
                            })
                ).await
            {
                Ok(_) => {
                    return;
                }
                Err(_e) => {
                    log::error!("error sending owner/repo check failure message: {:?}", _e);
                    return;
                }
            }
        }
        Some(gm) => {
            profile_data = Some(gm.payload);
        }
    }

    let user_name = options.get(2).and_then(|opt| {
        match &opt.resolved {
            Some(CommandDataOptionValue::String(s)) => Some(s.as_str()),
            _ => None,
        }
    });

    if let Some(user_name) = user_name {
        match is_code_contributor(owner, repo, user_name).await {
            false =>
                match
                    client.edit_original_interaction_response(
                        &ac.token,
                        &serde_json::json!({
                "content": format!("{user_name} hasn't contributed code to {owner}/{repo}. Bot will try to find out {user_name}'s other contributions."),
            })
                    ).await
                {
                    Ok(_) => {}
                    Err(_e) => {
                        log::error!(
                            "error sending is_code_contributor check failure message: {:?}",
                            _e
                        );
                    }
                }

            true => {}
        }
    } else {
        match
            client.edit_original_interaction_response(
                &ac.token,
                &serde_json::json!({
"content": format!("You didn't input a user's name. Bot will then create a report on the weekly progress of {owner}/{repo}."),
})
            ).await
        {
            Ok(_) => {}
            Err(_e) => {
                log::error!("error sending no-user_name acknowledgement message: {:?}", _e);
            }
        }
    }

    let addressee_str = user_name.map_or(String::from("key community participants'"), |n| {
        format!("{n}'s")
    });

    let start_msg_str = format!(
        "exploring {addressee_str} GitHub contributions to `{owner}/{repo}` project"
    );
    match
        client.edit_original_interaction_response(
            &ac.token,
            &serde_json::json!({
                "content": start_msg_str
            })
        ).await
    {
        Ok(_) => {}
        Err(_e) => log::error!("error sending start_msg_str: {:?}", _e),
    }

    let mut commits_summaries = String::from("");

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

            match
                client.edit_original_interaction_response(
                    &ac.token,
                    &serde_json::json!({
                        "content": commits_msg_str
                    })
                ).await
            {
                Ok(_) => {}
                Err(_e) => log::error!("error sending commit count: {:?}", _e),
            }

            match process_commits(commits_vec).await {
                Some((a, _, _)) => {
                    commits_summaries = a;
                }
                None => {}
            };
        }
        None => {}
    }

    let mut issues_summaries = String::from("");

    match get_issues_in_range(&owner, &repo, user_name, n_days).await {
        Some((count, issue_vec)) => {
            let issues_str = issue_vec
                .iter()
                .map(|issue| issue.url.rsplitn(2, '/').nth(0).unwrap_or("1234"))
                .collect::<Vec<&str>>()
                .join(", ");
            let issues_msg_str = format!("found {} issues: {}", count, issues_str);

            match
                client.edit_original_interaction_response(
                    &ac.token,
                    &serde_json::json!({
                        "content": issues_msg_str
                    })
                ).await
            {
                Ok(_) => {}
                Err(_e) => log::error!("error sending issues count: {:?}", _e),
            }

            match process_issues(issue_vec, user_name).await {
                Some((summary, _, _)) => {
                    issues_summaries = summary;
                }
                None => {}
            };
        }
        None => {} // Handle the case where get_issues_in_range returns None if needed
    }

    let now = Utc::now();
    let a_week_ago = now - Duration::days(n_days as i64);
    let a_week_ago_str = a_week_ago.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let discussion_query = format!("involves:{} updated:>{}", user_name.unwrap(), a_week_ago_str);

    let mut discussion_data = String::from("");

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

            match
                client.edit_original_interaction_response(
                    &ac.token,
                    &serde_json::json!({
                        "content": discussions_msg_str
                    })
                ).await
            {
                Ok(_) => {}
                Err(_e) => log::error!("error sending discussions message: {:?}", _e),
            }

            discussion_data = analyze_discussions(discussion_vec, user_name).await.0;
        }
        None => {} // Handle the case where search_discussions returns None if needed
    }

    let resp_content = correlate_commits_issues_discussions(
        Some(&commits_summaries),
        Some(&issues_summaries),
        Some(&discussion_data),
        user_name
    ).await.unwrap_or("Failed to generate report.".to_string());
    // let head = resp_content.chars().take(1000).collect::<String>();
    // send_message_to_channel("ik8", "ch_home", head).await;

    match
        client.edit_original_interaction_response(
            &ac.token,
            &serde_json::json!({
                "content": resp_content.to_string()
            })
        ).await
    {
        Ok(_) => {}
        Err(_e) => log::error!("error sending weekly_report message: {:?}", _e),
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
