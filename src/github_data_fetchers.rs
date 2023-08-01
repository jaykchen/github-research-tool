use crate::utils::*;
use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use github_flows::{
    get_octo,
    octocrab::{models::issues::Issue, Error as OctoError},
    GithubLogin,
};
use log;
use http_req::{request::Method, request::Request, uri::Uri};
use serde::{Deserialize, Serialize};
use serde_json;
use std::env;
pub async fn get_contributors(owner: &str, repo: &str) -> Option<Vec<String>> {
    #[derive(Debug, Deserialize)]
    struct GithubUser {
        login: String,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let url = format!("https://api.github.com/repos/{owner}/{repo}/contributors");
    let mut contributors = Vec::new();
    let mut next_url = Some(url.to_owned());

    while let Some(url) = next_url {
        match github_http_fetch_next(&github_token, &url).await {
            None => {
                log::error!("Error fetching contributors");
                return None;
            }
            Some((res, link)) => {
                let new_contributors: Vec<GithubUser> = match serde_json::from_slice(&res) {
                    Ok(contributors) => contributors,
                    Err(err) => {
                        log::error!("Error parsing contributors: {:?}", err);
                        return None;
                    }
                };

                contributors.extend(new_contributors.into_iter().map(|user| user.login));
                next_url = link;
            }
        }
    }

    Some(contributors)
}

pub async fn github_http_fetch_next(token: &str, url: &str) -> Option<(Vec<u8>, Option<String>)> {
    let url = Uri::try_from(url).unwrap();
    let mut writer = Vec::new();

    match Request::new(&url)
        .method(Method::GET)
        .header("User-Agent", "flows-network connector")
        .header("Content-Type", "application/vnd.github.v3+json")
        .header("Authorization", &format!("Bearer {}", token))
        .send(&mut writer)
    {
        Ok(res) => {
            if !res.status_code().is_success() {
                log::error!("Github http error {:?}", res.status_code());
                return None;
            };

            // Parse the Link header for the link to the next page
            let link_header = res.headers().get("Link").and_then(|header_value| Some(header_value.as_str()));
            let next_link = link_header.and_then(|header| {
                header.split(',').find_map(|link| {
                    if link.contains("rel=\"next\"") {
                        link.split(';').next().map(|url| url.trim_matches(&[' ', '<', '>'] as &[char]).to_owned())
                    } else {
                        None
                    }
                })
            });

            Some((writer, next_link))
        }
        Err(_e) => {
            log::error!("Error getting response from Github: {:?}", _e);
            None
        }
    }
}

// pub async fn get_contributors(owner: &str, repo: &str) -> Option<Vec<String>> {
//     #[derive(Debug, Deserialize)]
//     struct GithubUser {
//         login: String,
//     }

//     let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
//     let url = format!("https://api.github.com/repos/{owner}/{repo}/contributors");
//     match github_http_fetch(&github_token, &url).await {
//         None => {
//             log::error!("Error fetching contributors");
//             None
//         }
//         Some(res) => {
//             let contributors: Vec<GithubUser> = match serde_json::from_slice(&res) {
//                 Ok(contributors) => contributors,
//                 Err(err) => {
//                     log::error!("Error parsing contributors: {:?}", err);
//                     return None;
//                 }
//             };

//             Some(contributors.into_iter().map(|user| user.login).collect())
//         }
//     }
// }

pub async fn get_issues(owner: &str, repo: &str, user: &str) -> Option<Vec<Issue>> {
    #[derive(Debug, Deserialize)]
    struct Page<T> {
        pub items: Vec<T>,
        // pub incomplete_results: Option<bool>,
        pub total_count: Option<u64>,
        // pub next: Option<String>,
        // pub prev: Option<String>,
        // pub first: Option<String>,
        // pub last: Option<String>,
    }
    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let query = format!("repo:{owner}/{repo} involves:{user}");
    let encoded_query = urlencoding::encode(&query);

    let mut out: Vec<Issue> = vec![];
    let mut total_pages = None;
    for page in 1..=3 {
        if page > total_pages.unwrap_or(3) {
            break;
        }

        let url_str = format!(
            "https://api.github.com/search/issues?q={encoded_query}&sort=created&order=desc&page={page}"
        );

        match github_http_fetch(&github_token, &url_str).await {
            Some(res) => match serde_json::from_slice::<Page<Issue>>(res.as_slice()) {
                Err(_e) => log::error!("Error parsing Page<Issue>: {:?}", _e),

                Ok(issue_page) => {
                    if total_pages.is_none() {
                        if let Some(count) = issue_page.total_count {
                            total_pages = Some((count / 30) as usize + 1);
                        }
                    }
                    for issue in issue_page.items {
                        out.push(issue);
                    }
                }
            },

            None => {}
        }
    }

    Some(out)
}

/*
pub fn get_metrics(owner: &str, repo: &str) -> Option<String> {
    let octocrab = get_octo(&GithubLogin::Default);

    let metrics = octocrab
        .repos(owner, repo)
        .get_community_profile_metrics()
        .await;

    match metrics {
        Ok(page) => {
            let description = page.description.as_ref().unwrap();
            let documentation = page.documentation.as_ref().unwrap();
            let files = page
                .files
                .into_iter()
                .map(|(key, val)| val.map(|f| f.name).collect::<Vec<_>>())
                .collect::<Vec<_>>();

            log::error!("Github http error {:?}", res.status_code());
            return None;

            return Some(writer);
        }
        Err(_e) => {
            log::error!("Error getting response from Github: {:?}", _e);
        }
    }
}

pub fn get_user_profile(owner: &str, repo: &str) -> Option<String> {
    let octocrab = get_octo(&GithubLogin::Default);

    let metrics = octocrab
        .repos(owner, repo)
        .get_community_profile_metrics()
        .await;

    match metrics {
        Ok(page) => {
            let description = page.description.as_ref().unwrap();
            let documentation = page.documentation.as_ref().unwrap();
            let files = page
                .files
                .into_iter()
                .map(|(key, val)| val.map(|f| f.name).collect::<Vec<_>>())
                .collect::<Vec<_>>();

            log::error!("Github http error {:?}", res.status_code());
            return None;

            return Some(writer);
        }
        Err(_e) => {
            log::error!("Error getting response from Github: {:?}", _e);
        }
    }


}
 */

pub async fn search_mention(search_query: &str, search_type: Option<&str>) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct Root {
        data: Data,
    }

    #[derive(Debug, Deserialize)]
    struct Data {
        search: Search,
    }

    #[derive(Debug, Deserialize)]
    struct Search {
        edges: Vec<Edge>,
    }

    #[derive(Debug, Deserialize)]
    struct Edge {
        node: Node,
    }

    #[derive(Debug, Deserialize)]
    struct Node {
        title: Option<String>,
        url: String,
        createdAt: String,
    }
    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());

    let types = if let Some(search_type) = search_type {
        let upper = search_type.to_uppercase();
        let title_case = match upper.as_str() {
            "REPOSITORY" => "Repository",
            "ISSUE" => "Issue",
            "PULL_REQUEST" => "Pull_Request",
            "DISCUSSION" => "Discussion",
            _ => unreachable!("Invalid search type"),
        }
        .to_string();
        vec![(upper, title_case)]
    } else {
        vec![
            ("REPOSITORY".to_string(), "Repository".to_string()),
            ("ISSUE".to_string(), "Issue".to_string()),
            ("PULL_REQUEST".to_string(), "Pull_Request".to_string()),
            ("DISCUSSION".to_string(), "Discussion".to_string()),
        ]
    };

    let base_url = "https://api.github.com/graphql";
    let mut out = String::new();

    for search_type in &types {
        let query = format!(
            r#"
            query {{
                search(query: "{}", type: {}, first: 100) {{
                    edges {{
                        node {{
                            ... on {} {{
                                title
                                url
                                createdAt
                            }}
                        }}
                    }}
                }}
            }}
            "#,
            search_query, search_type.0, search_type.1
        );

        match github_http_post(&github_token, base_url, &query).await {
            None => log::error!("Failed to send the request to {}", base_url.to_string()),
            Some(response) => match serde_json::from_slice::<Root>(response.as_slice()) {
                Err(e) => log::error!("Failed to parse the response: {}", e),
                Ok(results) => {
                    log::info!(
                        "Found {} {}",
                        results.data.search.edges.len(),
                        search_type.1
                    );
                    for edge in results.data.search.edges {
                        let temp = format!(
                            "Type: {}, Title: {}, Url: {}, Created At: {}",
                            search_type.1,
                            edge.node.title.unwrap_or_default(),
                            edge.node.url,
                            edge.node.createdAt
                        );
                        out.push_str(&temp);
                    }
                }
            },
        };
    }

    Some(out)
}
pub async fn get_user_repos(user_name: &str, language: &str) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct Root {
        data: Data,
    }

    #[derive(Debug, Deserialize)]
    struct Data {
        search: Search,
    }

    #[derive(Debug, Deserialize)]
    struct Search {
        nodes: Vec<Node>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Node {
        pub name: String,
        pub defaultBranchRef: BranchRef,
        pub stargazers: Stargazers,
        pub description: Option<String>,
    }
    #[derive(Debug, Deserialize)]
    struct BranchRef {
        target: Target,
    }

    #[derive(Debug, Deserialize)]
    struct Target {
        history: History,
    }

    #[derive(Debug, Deserialize)]
    struct History {
        totalCount: i32,
    }

    #[derive(Debug, Deserialize)]
    struct Stargazers {
        totalCount: i32,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let query = format!(
        r#"
    query {{
        search(query: "user:{} language:{}", type: REPOSITORY, first: 100) {{
            nodes {{
                ... on Repository {{
                    name
                    defaultBranchRef {{
                        target {{
                            ... on Commit {{
                                history(first: 0) {{
                                    totalCount
                                }}
                            }}
                        }}
                    }}
                    description
                    stargazers {{
                        totalCount
                    }}
                }}
            }}
        }}
    }}
    "#,
        user_name, language
    );

    let base_url = "https://api.github.com/graphql";
    let mut out = String::new();
    match github_http_post(&github_token, base_url, &query).await {
        None => log::error!("Failed to send the request to {}", base_url.to_string()),
        Some(response) => match serde_json::from_slice::<Root>(response.as_slice()) {
            Err(e) => log::error!("Failed to parse the response: {}", e),
            Ok(repos) => {
                let mut repos_sorted: Vec<&Node> = repos.data.search.nodes.iter().collect();
                repos_sorted.sort_by(|a, b| b.stargazers.totalCount.cmp(&a.stargazers.totalCount));

                for repo in repos_sorted {
                    let temp = format!(
                        "Repo: {}, Description: {}, Stars: {}, Commits: {}",
                        repo.name,
                        repo.description.clone().unwrap_or_default(),
                        repo.stargazers.totalCount,
                        repo.defaultBranchRef.target.history.totalCount
                    );
                    out.push_str(&temp);
                }

                println!("Found {} repositories", repos.data.search.nodes.len());
            }
        },
    };
    Some(out)
}
pub async fn get_user_repos_octo(user_name: &str, language: &str) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct Root {
        data: Data,
    }

    #[derive(Debug, Deserialize)]
    struct Data {
        search: Search,
    }

    #[derive(Debug, Deserialize)]
    struct Search {
        nodes: Vec<Node>,
    }

    #[derive(Debug, Deserialize)]
    struct Node {
        name: String,
        defaultBranchRef: BranchRef,
        stargazers: Stargazers,
    }

    #[derive(Debug, Deserialize)]
    struct BranchRef {
        target: Target,
    }

    #[derive(Debug, Deserialize)]
    struct Target {
        history: History,
    }

    #[derive(Debug, Deserialize)]
    struct History {
        totalCount: i32,
    }

    #[derive(Debug, Deserialize)]
    struct Stargazers {
        totalCount: i32,
    }
    let octocrab = get_octo(&GithubLogin::Default);
    let query = format!(
        r#"
    query {{
        search(query: "user:{} language:{}", type: REPOSITORY, first: 100) {{
            nodes {{
                ... on Repository {{
                    name
                    defaultBranchRef {{
                        target {{
                            ... on Commit {{
                                history(first: 0) {{
                                    totalCount
                                }}
                            }}
                        }}
                    }}
                    stargazers {{
                        totalCount
                    }}
                }}
            }}
        }}
    }}
    "#,
        user_name, language
    );

    let res: Result<Root, OctoError> = octocrab
        .graphql(&serde_json::json! ({
            "query": query
        }))
        .await;

    let mut out = String::new();
    match res {
        Err(_e) => log::error!("Failed to send the request to {}", _e.to_string()),
        Ok(response) => {
            let mut repos_sorted: Vec<&Node> = response.data.search.nodes.iter().collect();
            repos_sorted.sort_by(|a, b| b.stargazers.totalCount.cmp(&a.stargazers.totalCount));

            for repo in repos_sorted {
                let temp = format!(
                    "Repo: {}, Stars: {}, Commits: {}",
                    repo.name,
                    repo.stargazers.totalCount,
                    repo.defaultBranchRef.target.history.totalCount
                );
                out.push_str(&temp);
            }

            log::error!("Found {} repositories", response.data.search.nodes.len());
        }
    };
    Some(out)
}
