use crate::utils::*;
use chrono::{DateTime, Utc};
use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use github_flows::{
    get_octo,
    octocrab::{models::issues::Issue, Error as OctoError},
    GithubLogin,
};
use http_req::{request::Method, request::Request, response::Response, uri::Uri};
use log;
use serde::{Deserialize, Serialize};
use serde_json;
use std::env;
use store_flows::{get, set};

pub async fn is_new_contributor(user_name: &str) -> bool {
    match get("usernames")
        .and_then(|val| serde_json::from_value::<std::collections::HashSet<String>>(val).ok())
    {
        Some(set) => !set.contains(user_name),
        None => true,
    }
}
pub async fn populate_contributors(owner: &str, repo: &str) -> (bool, u16) {
    match get_contributors(owner, repo).await {
        None => (false, 0_u16),

        Some(contributors) => {
            set(
                "contributors",
                serde_json::to_value(contributors).unwrap_or_default(),
                None,
            );

            match get("contributors").and_then(|val| {
                serde_json::from_value::<std::collections::HashSet<String>>(val).ok()
            }) {
                Some(set) => (true, set.len() as u16),
                None => {
                    log::error!("Error verifying contributors data in store");
                    (false, 0_u16)
                }
            }
        }
    }
}
pub async fn get_contributors(owner: &str, repo: &str) -> Option<Vec<String>> {
    #[derive(Debug, Deserialize)]
    struct GithubUser {
        login: String,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let url = format!(
        "https://api.github.com/repos/{}/{}/contributors",
        owner, repo
    );
    let mut contributors = Vec::new();

    let mut current_url = url.to_owned();
    loop {
        let response_result: Result<(Response, Vec<u8>), Box<dyn std::error::Error>> =
            github_fetch_with_header(&github_token, &current_url);
        match response_result {
            Err(e) => {
                log::error!(
                    "Error getting response for request to get contributors: {:?}",
                    e
                );
                return None;
            }
            Ok((res, body)) => {
                let status = res.status_code(); // Check the status code
                if !status.is_success() {
                    log::error!(
                        "Request to get contributors, unexpected status code: {:?}",
                        status
                    );
                    return None;
                }

                let new_contributors: Vec<GithubUser> = match serde_json::from_slice(body.as_slice()) {
                    Ok(contributors) => contributors,
                    Err(err) => {
                        log::error!("Error parsing contributors: {:?}", err);
                        return None;
                    }
                };

                contributors.extend(new_contributors.into_iter().map(|user| user.login));

                // Handle pagination
                let res_headers = res.headers();
                let link_header = res_headers.get("Link");
                match link_header {
                    Some(header) => {
                        let next_link_temp: Option<String> = header
                            .as_str()
                            .split(',')
                            .filter_map(|link| {
                                if link.contains("rel=\"next\"") {
                                    link.split(';').next().map(|url| {
                                        url.trim_matches(&[' ', '<', '>'] as &[char]).to_string()
                                    })
                                } else {
                                    None
                                }
                            })
                            .next();

                        let next_link = next_link_temp.as_deref();

                        if let Some(link) = next_link {
                            current_url = link.to_string();
                        } else {
                            break;
                        }
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    Some(contributors)
}

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

                log::info!("Found {} repositories", repos.data.search.nodes.len());
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

pub async fn search_issue(search_query: &str) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct Issue {
        title: Option<String>,
        url: Option<String>,
        createdAt: Option<DateTime<Utc>>,
    }

    #[derive(Debug, Deserialize)]
    struct IssueNode {
        node: Option<Issue>,
    }

    #[derive(Debug, Deserialize)]
    struct IssueEdge {
        edges: Vec<IssueNode>,
    }

    #[derive(Debug, Deserialize)]
    struct IssueSearch {
        search: IssueEdge,
    }

    #[derive(Debug, Deserialize)]
    struct IssueRoot {
        data: IssueSearch,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let base_url = "https://api.github.com/graphql";
    let mut out = String::from("ISSUES \n");

    let query = format!(
        r#"
        query {{
            search(query: "{search_query}", type: ISSUE, first: 100) {{
                edges {{
                    node {{
                        ... on Issue {{
                            title
                            url
                            createdAt
                        }}
                    }}
                }}
            }}
        }}
        "#
    );

    match github_http_post(&github_token, base_url, &query).await {
        None => log::error!("Failed to send the request: {}", base_url),
        Some(response) => match serde_json::from_slice::<IssueRoot>(response.as_slice()) {
            Err(e) => log::error!("Failed to parse the response: {}", e),
            Ok(results) => {
                for edge in results.data.search.edges {
                    match edge.node {
                        Some(issue) => {
                            let date = match issue.createdAt {
                                Some(date) => date.date_naive().to_string(),
                                None => continue,
                            };
                            let temp = format!(
                                "Title: {}, Url: {}, Created At: {}",
                                issue.title.unwrap_or("".to_string()),
                                issue.url.unwrap_or("".to_string()),
                                date,
                            );
                            out.push_str(&temp);
                        }
                        None => continue,
                    }
                }
            }
        },
    };

    Some(out)
}

pub async fn search_repository(search_query: &str) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct StarGazers {
        totalCount: i32,
    }

    #[derive(Debug, Deserialize)]
    struct Repository {
        name: Option<String>,
        description: Option<String>,
        url: Option<String>,
        createdAt: Option<DateTime<Utc>>,
        stargazers: Option<StarGazers>,
        forkCount: Option<i32>,
    }

    #[derive(Debug, Deserialize)]
    struct RepositoryNode {
        node: Option<Repository>,
    }

    #[derive(Debug, Deserialize)]
    struct RepositoryEdge {
        edges: Vec<RepositoryNode>,
    }

    #[derive(Debug, Deserialize)]
    struct RepositorySearch {
        search: RepositoryEdge,
    }

    #[derive(Debug, Deserialize)]
    struct RepositoryRoot {
        data: RepositorySearch,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let base_url = "https://api.github.com/graphql";
    let mut out = String::from("REPOSITORY \n");

    let query = format!(
        r#"query {{
                search(query: "{search_query}", type: REPOSITORY, first: 100) {{
                    edges {{
                        node {{
                            ... on Repository {{
                                name
                                description
                                url
                                createdAt
                                stargazers {{
                                  totalCount
                                }}
                                forkCount
                            }}
                        }}
                    }}
                }}
            }}
            "#
    );

    match github_http_post(&github_token, base_url, &query).await {
        None => log::error!(
            "Failed to send the request to get RepositoryRoot: {}",
            base_url
        ),
        Some(response) => match serde_json::from_slice::<RepositoryRoot>(response.as_slice()) {
            Err(e) => log::error!("Failed to parse the responsefor RepositoryRoot: {}", e),
            Ok(results) => {
                for edge in results.data.search.edges {
                    match edge.node {
                        Some(repo) => {
                            let date = match repo.createdAt {
                                Some(date) => date.date_naive().to_string(),
                                None => continue,
                            };
                            let stars = match repo.stargazers {
                                Some(s) => s.totalCount,
                                None => 0,
                            };
                            let forks = repo.forkCount.unwrap_or(0);
                            let temp = format!(
                                    "Name: {}, Description: {}, Url: {}, Created At: {}, Stars: {}, Forks: {}",
                                    repo.name.unwrap_or("".to_string()),
                                    repo.description.unwrap_or("".to_string()),
                                    repo.url.unwrap_or("".to_string()),
                                    date,
                                    stars,
                                    forks,
                                );
                            out.push_str(&temp);
                        }
                        None => continue,
                    }
                }
            }
        },
    };

    Some(out)
}

pub async fn search_discussion(search_query: &str) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct Discussion {
        title: Option<String>,
        url: Option<String>,
        createdAt: Option<DateTime<Utc>>,
        upvoteCount: Option<i32>,
    }

    #[derive(Debug, Deserialize)]
    struct DiscussionNode {
        node: Option<Discussion>,
    }

    #[derive(Debug, Deserialize)]
    struct DiscussionEdge {
        edges: Vec<DiscussionNode>,
    }

    #[derive(Debug, Deserialize)]
    struct DiscussionSearch {
        search: DiscussionEdge,
    }

    #[derive(Debug, Deserialize)]
    struct DiscussionRoot {
        data: DiscussionSearch,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let base_url = "https://api.github.com/graphql";
    let mut out = String::from("DISCUSSION: \n");

    let query = format!(
        r#"
        query {{
            search(query: "{search_query}", type: DISCUSSION, first: 100) {{
                edges {{
                    node {{
                        ... on Discussion {{
                            title
                            url
                            createdAt
                            upvoteCount
                        }}
                    }}
                }}
            }}
        }}
        "#
    );

    match github_http_post(&github_token, base_url, &query).await {
        None => {
            log::error!(
                "Failed to send the request to get DiscussionRoot: {}",
                base_url
            );
            return None;
        }
        Some(response) => match serde_json::from_slice::<DiscussionRoot>(response.as_slice()) {
            Err(e) => {
                log::error!("Failed to parse the response for DiscussionRoot: {}", e);
                return None;
            }
            Ok(results) => {
                for edge in results.data.search.edges {
                    if let Some(discussion) = edge.node {
                        let date = match discussion.createdAt {
                            Some(date) => date.date_naive(),
                            None => continue,
                        };

                        let temp = format!(
                            "Title: {}, Url: {}, Created At: {}, Upvotes: {}",
                            discussion.title.as_deref().unwrap_or(""),
                            discussion.url.as_deref().unwrap_or(""),
                            date,
                            discussion.upvoteCount.unwrap_or(0),
                        );
                        out.push_str(&temp);
                    }
                }
            }
        },
    };

    Some(out)
}
