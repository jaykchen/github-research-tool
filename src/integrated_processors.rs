use crate::data_analyzers::analyze_issue;
use crate::github_data_fetchers::get_issue_texts;
use crate::octocrab_compat::{Comment, Issue, Repository, User};
use crate::utils::*;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use derivative::Derivative;
use http_req::response::Response;
use serde::{Deserialize, Serialize};
use serde_json;
use store_flows::{get, set};

pub async fn get_issues_in_range_and_process(
    github_token: &str,
    owner: &str,
    repo: &str,
    user_name: Option<&str>,
    shared_meta: &mut Option<Vec<String>>,
    range: u16,
) -> Option<Vec<GitMemory>> {
    #[derive(Debug, Deserialize)]
    struct Page<T> {
        pub items: Vec<T>,
        pub total_count: Option<u64>,
    }

    let now = Utc::now();
    let n_days_ago = (now - Duration::days(range as i64))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    let user_str = user_name
        .map(|u| format!("involves:{}", u))
        .unwrap_or_default();

    let query = format!("repo:{owner}/{repo} is:issue {user_str} updated:>{n_days_ago}");
    let encoded_query = urlencoding::encode(&query);

    let mut issue_vec = vec![];
    let mut total_pages = None;
    let mut current_page = 1;
    let mut shared_meta_inner = Vec::new();
    loop {
        let url_str = format!(
            "https://api.github.com/search/issues?q={}&sort=updated&order=desc&page={}",
            encoded_query, current_page
        );

        match github_http_fetch(&github_token, &url_str).await {
            Some(res) => match serde_json::from_slice::<Page<Issue>>(res.as_slice()) {
                Err(e) => {
                    log::error!("error: {:?}", e);
                    break;
                }
                Ok(issue_page) => {
                    if total_pages.is_none() {
                        if let Some(total) = issue_page.total_count {
                            total_pages = Some(((total as f64) / 30.0).ceil() as usize);
                        }
                    }

                    for issue in issue_page.items {
                        shared_meta.push_str(format!("{date_str} {title_str} "));
                        let all_text = get_issue_texts(github_token, &issue).await.unwrap();

                        match analyze_issue(&issue, user_name, &all_text).await {
                            Some(summary, _) => issue_vec.push(GitMemory {
                                memory_type: MemoryType::Issue,
                                source_url: issue.url.unwrap_or(String::new()),
                                name: issue.user.login.clone(),
                                tag_line: format!("#{}: {}", issue.number, issue.title),
                                payload: summary,
                                date: issue.updated_at.date_naive(),
                            }),
                            None => {}
                        }
                    }

                    current_page += 1;
                    if current_page > total_pages.unwrap_or(usize::MAX) {
                        break;
                    }
                }
            },
            None => {
                break;
            }
        }
    }
    let count = issue_vec.len();
    Some(issue_vec)
}
