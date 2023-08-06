use crate::octocrab_compat::{Comment, Issue};
use crate::utils::*;
use crate::github_data_fetchers::*;
use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use log;
use serde::{Deserialize, Serialize};
use serde_json;
use std::env;

pub async fn weekly_report(
    owner: &str,
    repo: &str,
    user_name: Option<&str>,
    key: Option<&str>,
) -> Option<String> {
    let (success_or_fail, contributor_count) = populate_contributors(owner, repo).await;
    let key = match key {
        Some(key) => key,
        None => "contributors",
    };
    if is_new_contributor(user_name, key).await {
        
        let issues_data = search_issue(owner, repo, Some("is:issue is:open"), Some("comments,labels,participants")).await.unwrap();



    }

    return None;
}
