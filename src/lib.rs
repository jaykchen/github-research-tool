pub mod data_analyzers;
pub mod discord_functions;
pub mod github_data_fetchers;
pub mod octocrab_compat;
pub mod reports;
pub mod utils;
use chrono::{Duration, Utc};
use data_analyzers::*;
use discord_flows::model::interactions::application_command::{
    ApplicationCommand, ApplicationCommandInteraction,
};
use discord_flows::{
    http::Http,
    model::{application_command::CommandDataOptionValue, channel, guild, Interaction},
    Bot, EventModel, ProvidedBot,
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
    // let _ = register_once(&discord_token).await;

    let bot = ProvidedBot::new(discord_token);
    bot.listen(|em| handle(&bot, em)).await;
}

/* async fn handle<B: Bot>(bot: &B, em: EventModel) {
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
                "weekly_report" => {
                    handle_weekly_report(bot, ac).await;
                }
                "get_user_repos" => {
                    // handle_get_user_repos(bot, ac).await;
                }
                "search" => {
                    // handle_search(bot, ac).await;
                }
                _ => {}
            }
        }
        EventModel::Message(msg) => {
            // keep it empty for now
        }
    }
} */

async fn handle<B: Bot>(bot: &B, em: EventModel) {
    let client = bot.get_client();
    match em {
        EventModel::ApplicationCommand(ac) => {
            let initial_response = serde_json::json!(
                {
                    "type": 4,
                    "data": {
                        "content": "Bot is pulling data for you,🤖⏳ please wait."
                    }
                }
            );
            _ = client
                .create_interaction_response(ac.id.into(), &ac.token, &initial_response)
                .await;
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

    let user_name = options.get(2).and_then(|opt| match &opt.resolved {
        Some(CommandDataOptionValue::String(s)) => Some(s.as_str()),
        _ => None,
    });

    let (commits_count, commits_vec) = match get_commits_in_range(&owner, &repo, user_name, 7).await
    {
        Some(res) => res,
        None => (0, vec![]),
    };
    // let head = commits_vec[0]
    //     .payload
    //     .chars()
    //     .take(1000)
    //     .collect::<String>();
    // send_message_to_channel("ik8", "ch_rep", head).await;

    let resp = serde_json::json!({
        "content": format!("processing {} commits", commits_count)
    });

    match client
        .edit_original_interaction_response(&ac.token, &resp)
        .await
    {
        Ok(_) => {}
        Err(_e) => log::error!("error sending commit count: {:?}", _e),
    }

    let (commits_summaries, _, gm_vec) = match commits_vec.is_empty() {
        true => (String::from(""), 0, vec![]),
        false => match process_commits(commits_vec).await {
            Some(res) => res,
            None => (String::from(""), 0, vec![]),
        },
    };

    // let resp = serde_json::json!({
    //     "content": gm_vec[0].payload
    // });

    // match client
    //     .edit_original_interaction_response(&ac.token, &resp)
    //     .await
    // {
    //     Ok(_) => {}
    //     Err(_e) => log::error!("error sending commit summaries: {:?}", _e),
    // }
    // let head = commits_summaries.chars().take(1000).collect::<String>();
    // send_message_to_channel("ik8", "ch_rep", head).await;

    let (count, issue_vec) = match get_issues_in_range(&owner, &repo, user_name, 7).await {
        Some(res) => res,
        None => (0, vec![]),
    };

    let resp = serde_json::json!({
        "content": format!("{} issues pulled", count)
    });

    match client
        .edit_original_interaction_response(&ac.token, &resp)
        .await
    {
        Ok(_) => {}
        Err(_e) => log::error!("error sending issues count: {:?}", _e),
    }

    let (issues_summaries, _, _) = match issue_vec.is_empty() {
        true => (String::from(""), 0, vec![]),
        false => match process_issues(issue_vec, user_name).await {
            Some(res) => res,
            None => (String::from(""), 0, vec![]),
        },
    };

    // let head = issues_summaries.chars().take(1000).collect::<String>();
    // send_message_to_channel("ik8", "ch_iss", head).await;

    let now = Utc::now();
    let a_week_ago = now - Duration::days(7);
    let a_week_ago_str = a_week_ago.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let discussion_query = format!(
        "involves:{} updated:>{}",
        user_name.unwrap(),
        a_week_ago_str
    );

    let (discussion_count, discussion_vec) = match search_discussions(&discussion_query).await {
        Some(res) => res,
        None => (0, vec![]),
    };

    let resp = serde_json::json!({
        "content": format!("processing {} discussions", discussion_count)
    });

    match client
        .edit_original_interaction_response(&ac.token, &resp)
        .await
    {
        Ok(_) => {}
        Err(_e) => log::error!("error sending discussions count: {:?}", _e),
    }

    let (discussion_data, _) = match discussion_vec.is_empty() {
        true => (String::from(""), vec![]),
        false => analyze_discussions(discussion_vec, user_name).await,
    };
    // let head = discussion_data.chars().take(1000).collect::<String>();
    // send_message_to_channel("ik8", "ch_dis", head).await;

    let resp_content = correlate_commits_issues_discussions(
        &commits_summaries,
        &issues_summaries,
        &discussion_data,
    )
    .await
    .unwrap_or("Failed to generate report.".to_string());
    let head = resp_content.chars().take(1000).collect::<String>();
    send_message_to_channel("ik8", "ch_home", head).await;

    let resp = serde_json::json!({
        "content": resp_content.to_string()
    });

    match client
        .edit_original_interaction_response(&ac.token, &resp)
        .await
    {
        Ok(_) => {}
        Err(_e) => log::error!("error sending weekly_report message: {:?}", _e),
    }
}

async fn handle_search<B: Bot>(bot: &B, client: &Http, ac: ApplicationCommandInteraction) {
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
        "issue" => search_result = search_issue(&search_query).await.unwrap_or("".to_string()),
        "users" => search_result = search_users(&search_query).await.unwrap_or("".to_string()),
        "repository" => {
            search_result = search_repository(&search_query)
                .await
                .unwrap_or("".to_string())
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
    match client
        .edit_original_interaction_response(&ac.token, &resp)
        .await
    {
        Ok(_) => {}
        Err(_e) => log::error!("error sending search message: {:?}", _e),
    }
}

async fn handle_get_user_repos<B: Bot>(bot: &B, client: &Http, ac: ApplicationCommandInteraction) {
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

    let user_repos = get_user_repos_gql(&username, &language)
        .await
        .unwrap_or("Couldn't get any repos!".to_string());

    let resp = serde_json::json!({
        "content": user_repos.to_string()
    });

    match client
        .edit_original_interaction_response(&ac.token, &resp)
        .await
    {
        Ok(_) => {}
        Err(_e) => log::error!("error sending get_user_repos message: {:?}", _e),
    }
}
