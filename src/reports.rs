use crate::data_analyzers::*;
use crate::github_data_fetchers::*;
use chrono::{Duration, Utc};
use log;

pub async fn weekly_report(
    owner: &str,
    repo: &str,
    user_name: Option<&str>,
    key: Option<&str>,
) -> Option<String> {
    let key = match key {
        Some(key) => key,
        None => "contributors",
    };
    let (success_or_fail, contributor_count) = populate_contributors(owner, repo, key).await;
    if !success_or_fail {
        log::error!("weekly_report, failed to populate contributors");
        return None;
    }

    if let Some(user_name) = user_name {
        if is_new_contributor(user_name, key).await {
            let mut home_repo_data = get_readme(owner, repo).await.unwrap_or("".to_string());
            let home_repo_profile = get_community_profile(owner, repo)
                .await
                .unwrap_or("".to_string());
            home_repo_data.push_str(&home_repo_profile);
            let user_profile = get_user_profile(user_name).await.unwrap_or("".to_string());

            let now = Utc::now();
            let a_week_ago = now - Duration::days(7);
            let a_week_ago_str = a_week_ago.format("%Y-%m-%dT%H:%M:%SZ").to_string();

            let issue_query = format!("involves:{user_name} updated:>{a_week_ago_str}");
            let issues_data = search_issue(&issue_query).await.unwrap_or("".to_string());
            let mut repos_data = String::new();

            for language in vec!["rust", "javascript", "cplusplus"] {
                let temp = get_user_repos_gql(user_name, language)
                    .await
                    .unwrap_or("".to_string());
                repos_data.push_str(&temp);
            }

            let discussion_query = format!("involves:{user_name} updated:>{a_week_ago_str}");
            let discussion_data = search_discussion(&discussion_query)
                .await
                .unwrap_or("".to_string());

            return correlate_user_and_home_project(
                &home_repo_data,
                &user_profile,
                &issues_data,
                &repos_data,
                &discussion_data,
            )
            .await;
        } else {
            return Some("placeholder text for existing contributor".to_string());
        }
    }

    None // This is the default return for when user_name is None
}
