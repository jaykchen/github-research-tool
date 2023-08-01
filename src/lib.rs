pub mod data_analyzers;
pub mod github_data_fetchers;
pub mod utils;
use data_analyzers::*;
use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use github_data_fetchers::*;
use utils::*;
use log;
use slack_flows::{listen_to_channel, send_message_to_channel, SlackMessage};
use std::env;

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn run() {
    logger::init();
    dotenv().ok();

    let slack_workspace = env::var("slack_workspace").unwrap_or("secondstate".to_string());
    let slack_channel = env::var("slack_channel").unwrap_or("github-status".to_string());

    listen_to_channel(&slack_workspace, &slack_channel, |sm| {
        handler(&slack_workspace, &slack_channel, sm)
    })
    .await;
}

async fn handler(workspace: &str, channel: &str, sm: SlackMessage) {
    let trigger_word = env::var("trigger_word").unwrap_or("bot@get".to_string());

    let parts: Vec<&str> = sm
        .text
        .split(&trigger_word)
        .nth(1) // skip the part before "bot@get"
        .unwrap_or("") // if "bot@get" is not found, use an empty string
        .split_whitespace()
        .collect();

    let (owner, repo, user_name, language) = match parts.as_slice() {
        [owner, repo, user, language, ..] => (owner, repo, user, language),
        _ => panic!("Input should contain 'bot@get <github_owner> <github_repo> <user_name>'"),
    };

    log::info!("language: {:?}", language.to_string());
    if sm.text.contains(&trigger_word) {
        if let Some(res) = get_user_repos(user_name, language).await {
            log::info!("res: {:?}", res.clone());
            send_message_to_channel("ik8", "ch_in", res).await;
        }

        let search_query = user_name.to_string();
        if let Some(res) = search_mention(&search_query, Some("ISSUE")).await {
            log::info!("res: {:?}", res.clone());
            send_message_to_channel("ik8", "ch_mid", res).await;
        }

        if !save_user(user_name).await {
            send_message_to_channel("ik8", "ch_mid", format!("{user_name} is a new contributor"))
                .await;
        }
    }
}
