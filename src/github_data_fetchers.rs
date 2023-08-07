use crate::octocrab_compat::{Issue, Repository, User};
use crate::utils::*;
use chrono::{DateTime, Duration, Utc};
use http_req::response::Response;
use serde::Deserialize;
use serde_json;
use std::env;
use store_flows::{get, set};

pub async fn get_user_profile(user: &str) -> Option<User> {
    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let user_profile_url = format!("https://api.github.com/users/{user}");

    match github_http_fetch(&github_token, &user_profile_url).await {
        Some(res) => serde_json::from_slice::<User>(res.as_slice()).ok(),

        None => {
            log::error!("Github user not found.");
            None
        }
    }
}
pub async fn get_user_by_login_string(login: &str) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct User {
        name: Option<String>,
        login: Option<String>,
        url: Option<String>,
        twitterUsername: Option<String>,
        bio: Option<String>,
        company: Option<String>,
        location: Option<String>,
        createdAt: Option<DateTime<Utc>>,
        email: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct RepositoryOwner {
        repositoryOwner: Option<User>,
    }

    #[derive(Debug, Deserialize)]
    struct UserRoot {
        data: Option<RepositoryOwner>,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let base_url = "https://api.github.com/graphql";
    let mut out = String::from("USER_profile: \n");

    let query = format!(
        r#"
        query {{
            repositoryOwner(login: "{login}") {{
                ... on User {{
                    name
                    login
                    url
                    twitterUsername
                    bio
                    company
                    location
                    createdAt
                    email
                }}
            }}
        }}
        "#
    );

    match github_http_post(&github_token, base_url, &&query).await {
        None => {
            log::info!("Failed to send the request to get UserRoot: {}", base_url);
            return None;
        }
        Some(res) => match serde_json::from_slice::<UserRoot>(res.as_slice()) {
            Err(e) => {
                log::error!("Failed to parse the response for UserRoot: {}", e);
                return None;
            }
            Ok(results) => {
                if let Some(repository_owner) = &results.data {
                    if let Some(user) = &repository_owner.repositoryOwner {
                        let login_str = match &user.login {
                            Some(login) => format!("Login: {},", login),
                            None => return None,
                        };

                        let name_str = match &user.name {
                            Some(name) => format!("Name: {},", name),
                            None => String::new(),
                        };

                        let url_str = match &user.url {
                            Some(url) => format!("Url: {},", url),
                            None => String::new(),
                        };

                        let twitter_str = match &user.twitterUsername {
                            Some(twitter) => format!("Twitter: {},", twitter),
                            None => String::new(),
                        };

                        let bio_str = match &user.bio {
                            Some(bio) if bio.is_empty() => String::new(),
                            Some(bio) => format!("Bio: {},", bio),
                            None => String::new(),
                        };

                        let company_str = match &user.company {
                            Some(company) => format!("Company: {},", company),
                            None => String::new(),
                        };

                        let location_str = match &user.location {
                            Some(location) => format!("Location: {},", location),
                            None => String::new(),
                        };

                        let date_str = match &user.createdAt {
                            Some(date) => {
                                format!("Created At: {},", date.date_naive().to_string())
                            }
                            None => String::new(),
                        };

                        let email_str = match &user.email {
                            Some(email) => format!("Email: {}", email),
                            None => String::new(),
                        };

                        out.push_str(&format!(
                            "{name_str} {login_str} {url_str} {twitter_str} {bio_str} {company_str} {location_str} {date_str} {email_str}\n"
                        ));
                    }
                }
            }
        },
    };

    Some(out)
}
pub async fn get_community_profile_string(owner: &str, repo: &str) -> Option<String> {
    #[derive(Deserialize, Debug)]
    struct CommunityProfile {
        description: String,
        // documentation: Option<String>,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let community_profile_url =
        format!("https://api.github.com/repos/{owner}/{repo}/community/profile");

    match github_http_fetch(&github_token, &community_profile_url).await {
        Some(res) => match serde_json::from_slice::<CommunityProfile>(&res) {
            Ok(profile) => {
                return Some(format!("Description: {}", profile.description));
            }
            Err(e) => log::error!("Error parsing Community Profile: {:?}", e),
        },
        None => log::error!("Community profile not found for {}/{}.", owner, repo),
    }
    None
}

pub async fn get_readme(owner: &str, repo: &str) -> Option<String> {
    #[derive(Deserialize, Debug)]
    struct GithubReadme {
        // size: usize,
        // url: String,
        // html_url: String,
        content: String,
        // encoding: String,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let readme_url = format!("https://api.github.com/repos/{owner}/{repo}/readme");
    match github_http_fetch(&github_token, &readme_url).await {
        Some(res) => match serde_json::from_slice::<GithubReadme>(&res) {
            Ok(readme) => {
                let cleaned_content = readme.content.replace("\n", "");
                match base64::decode(&cleaned_content) {
                    Ok(decoded_content) => {
                        match &String::from_utf8(decoded_content) {
                            Ok(out) if out.len() > 3000 => {
                                let truncated = out
                                    .chars()
                                    .take(1800)
                                    .chain(out.chars().skip(out.chars().count() - 1200))
                                    .collect::<String>();

                                return Some(format!("Readme: {truncated}"));
                            }
                            Ok(out) => return Some(format!("Readme: {out}")),
                            Err(_e) => {
                                log::error!("failed to convert cleaned readme to String: {_e}");
                                return None;
                            }
                        };
                    }
                    Err(_) => log::error!("Error decoding base64 content."),
                }
            }
            Err(e) => log::error!("Error parsing Readme: {:?}", e),
        },
        None => log::error!("Github readme not found."),
    }
    None
}

pub async fn is_new_contributor(user_name: &str, key: &str) -> bool {
    match get(key)
        .and_then(|val| serde_json::from_value::<std::collections::HashSet<String>>(val).ok())
    {
        Some(set) => !set.contains(user_name),
        None => true,
    }
}
pub async fn populate_contributors(owner: &str, repo: &str, key: &str) -> (bool, u16) {
    match get_contributors(owner, repo).await {
        None => (false, 0_u16),

        Some(contributors) => {
            set(
                key,
                serde_json::to_value(contributors).unwrap_or_default(),
                None,
            );

            match get(key).and_then(|val| {
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

                let new_contributors: Vec<GithubUser> =
                    match serde_json::from_slice(body.as_slice()) {
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

pub async fn get_user_issues_on_repo_last_n_days(
    owner: &str,
    repo: &str,
    user: &str,
    n_days: u16,
) -> Option<Vec<Issue>> {
    #[derive(Debug, Deserialize)]
    struct Page<T> {
        pub items: Vec<T>,
        pub total_count: Option<u64>,
    }
    let now = Utc::now();

    let n_days_ago = now - Duration::days(n_days.into());
    let n_days_ago_str = n_days_ago.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let query = format!("repo:{owner}/{repo} involves:{user} updated:>{n_days_ago_str}");
    let encoded_query = urlencoding::encode(&query);

    let mut out: Vec<Issue> = vec![];
    let mut total_pages = None;
    let mut current_page = 1;
    let mut count = 0;
    loop {
        let url_str = format!(
            "https://api.github.com/search/issues?q={encoded_query}&sort=created&order=desc&page={current_page}"
        );

        match github_http_fetch(&github_token, &url_str).await {
            Some(res) => match serde_json::from_slice::<Page<Issue>>(res.as_slice()) {
                Err(_e) => {
                    log::error!("Error parsing Page<Issue>: {:?}", _e);
                    break;
                }
                Ok(issue_page) => {
                    if total_pages.is_none() {
                        if let Some(count) = issue_page.total_count {
                            total_pages = Some((count as f64 / 30.0).ceil() as usize);
                        }
                    }

                    for issue in issue_page.items {
                        out.push(issue);
                        count += 1;

                        if count > 1 {
                            break;
                        }
                    }

                    current_page += 1;
                    if current_page > total_pages.unwrap_or(usize::MAX) {
                        break;
                    }
                }
            },
            None => break,
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub async fn get_user_issues_on_repo(owner: &str, repo: &str, user: &str) -> Option<Vec<Issue>> {
    #[derive(Debug, Deserialize)]
    struct Page<T> {
        pub items: Vec<T>,
        pub total_count: Option<u64>,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let query = format!("repo:{owner}/{repo} involves:{user}");
    let encoded_query = urlencoding::encode(&query);

    let mut out: Vec<Issue> = vec![];
    let mut total_pages = None;
    let mut current_page = 1;
    let mut count = 0;
    loop {
        let url_str = format!(
            "https://api.github.com/search/issues?q={encoded_query}&sort=created&order=desc&page={current_page}"
        );

        match github_http_fetch(&github_token, &url_str).await {
            Some(res) => match serde_json::from_slice::<Page<Issue>>(res.as_slice()) {
                Err(_e) => {
                    log::error!("Error parsing Page<Issue>: {:?}", _e);
                    break;
                }
                Ok(issue_page) => {
                    if total_pages.is_none() {
                        if let Some(count) = issue_page.total_count {
                            total_pages = Some((count as f64 / 30.0).ceil() as usize);
                        }
                    }

                    for issue in issue_page.items {
                        out.push(issue);
                        count += 1;

                        if count > 99 {
                            break;
                        }
                    }

                    current_page += 1;
                    if current_page > total_pages.unwrap_or(usize::MAX) {
                        break;
                    }
                }
            },
            None => break,
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub async fn get_user_repos_in_language(user: &str, language: &str) -> Option<Vec<Repository>> {
    #[derive(Debug, Deserialize)]
    struct Page<T> {
        pub items: Vec<T>,
        pub total_count: Option<u64>,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let query = format!("user:{} language:{} sort:stars", user, language);
    let encoded_query = urlencoding::encode(&query);

    let mut out: Vec<Repository> = vec![];
    let mut total_pages = None;
    let mut current_page = 1;

    loop {
        let url_str = format!(
            "https://api.github.com/search/repositories?q={}&page={}",
            encoded_query, current_page
        );

        match github_http_fetch(&github_token, &url_str).await {
            Some(res) => match serde_json::from_slice::<Page<Repository>>(res.as_slice()) {
                Err(_e) => {
                    log::error!("Error parsing Page<Repository>: {:?}", _e);
                    break;
                }
                Ok(repo_page) => {
                    if total_pages.is_none() {
                        if let Some(count) = repo_page.total_count {
                            total_pages = Some((count as f64 / 30.0).ceil() as usize);
                        }
                    }

                    for repo in repo_page.items {
                        out.push(repo);
                    }

                    current_page += 1;
                    if current_page > total_pages.unwrap_or(usize::MAX) {
                        break;
                    }
                }
            },
            None => break,
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub async fn get_user_repos_gql(user_name: &str, language: &str) -> Option<String> {
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
    let mut out = format!("Repos in {language}:\n");
    match github_http_post(&github_token, base_url, &query).await {
        None => log::error!("Failed to send the request to {}", base_url.to_string()),
        Some(response) => match serde_json::from_slice::<Root>(response.as_slice()) {
            Err(e) => log::error!("Failed to parse the response: {}", e),
            Ok(repos) => {
                let mut repos_sorted: Vec<&Node> = repos.data.search.nodes.iter().collect();
                repos_sorted.sort_by(|a, b| b.stargazers.totalCount.cmp(&a.stargazers.totalCount));

                for repo in repos_sorted {
                    let name_str = format!("Repo: {}", repo.name);

                    let description_str = match &repo.description {
                        Some(description) => format!("Description: {},", description),
                        None => String::new(),
                    };

                    let stars_str = match repo.stargazers.totalCount {
                        0 => String::new(),
                        count => format!("Stars: {count}"),
                    };

                    let commits_str = format!(
                        "Commits: {}",
                        repo.defaultBranchRef.target.history.totalCount
                    );

                    let temp = format!("{name_str} {description_str} {stars_str} {commits_str}\n");

                    out.push_str(&temp);
                }

                log::info!("Found {} repositories", repos.data.search.nodes.len());
            }
        },
    };
    Some(out)
}

pub async fn search_issue(search_query: &str) -> Option<String> {
    #[derive(Debug, Deserialize, Clone)]
    pub struct User {
        login: Option<String>,
    }

    #[derive(Debug, Deserialize, Clone)]
    struct AssigneeNode {
        node: Option<User>,
    }

    #[derive(Debug, Deserialize, Clone)]
    struct AssigneeEdge {
        edges: Option<Vec<Option<AssigneeNode>>>,
    }

    #[derive(Debug, Deserialize, Clone)]
    struct Issue {
        url: Option<String>,
        number: Option<u64>,
        state: Option<String>,
        title: Option<String>,
        body: Option<String>,
        author: Option<User>,
        assignees: Option<AssigneeEdge>,
        authorAssociation: Option<String>,
        createdAt: Option<DateTime<Utc>>,
        updatedAt: Option<DateTime<Utc>>,
    }

    #[derive(Debug, Deserialize)]
    struct IssueNode {
        node: Option<Issue>,
    }

    #[derive(Debug, Deserialize, Clone)]
    struct PageInfo {
        endCursor: Option<String>,
        hasNextPage: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    struct SearchResult {
        edges: Option<Vec<Option<IssueNode>>>,
        pageInfo: Option<PageInfo>,
    }

    #[derive(Debug, Deserialize)]
    struct IssueSearch {
        search: Option<SearchResult>,
    }

    #[derive(Debug, Deserialize)]
    struct IssueRoot {
        data: Option<IssueSearch>,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let base_url = "https://api.github.com/graphql";
    let mut out = String::from("ISSUES \n");

    let mut cursor = None;

    loop {
        let query = format!(
            r#"
            query {{
                search(query: "{search_query}", type: ISSUE, first: 100{after}) {{
                    edges {{
                        node {{
                            ... on Issue {{
                                url
                                number
                                state
                                title
                                body
                                author {{
                                    login
                                }}
                                assignees(first: 100) {{
                                    edges {{
                                        node {{
                                            login
                                        }}
                                    }}
                                }}
                                authorAssociation
                                createdAt
                                updatedAt
                            }}
                        }}
                    }}
                    pageInfo {{
                        endCursor
                        hasNextPage
                      }}
                }}
            }}
            "#,
            search_query = search_query,
            after = cursor
                .as_ref()
                .map_or(String::new(), |c| format!(r#", after: "{}""#, c)),
        );

        match github_http_post(&github_token, base_url, &query).await {
            None => {
                log::error!("Failed to send the request: {}", base_url);
                break;
            }
            Some(response) => match serde_json::from_slice::<IssueRoot>(response.as_slice()) {
                Err(e) => {
                    log::error!("Failed to parse the response: {}", e);
                    break;
                }
                Ok(results) => {
                    if let Some(search) = &results.data.as_ref().and_then(|d| d.search.as_ref()) {
                        if let Some(edges) = &search.edges {
                            for edge in edges.iter().filter_map(|e| e.as_ref()) {
                                if let Some(issue) = &edge.node {
                                    let date = match issue.createdAt {
                                        Some(date) => date.date_naive().to_string(),
                                        None => continue,
                                    };
                                    let title_str = match &issue.title {
                                        Some(title) => format!("Title: {},", title),
                                        None => String::new(),
                                    };
                                    let url_str = match &issue.url {
                                        Some(u) => format!("Url: {}", u),
                                        None => String::new(),
                                    };

                                    let author_str =
                                        match issue.clone().author.and_then(|a| a.login) {
                                            Some(auth) => format!("Author: {},", auth),
                                            None => String::new(),
                                        };

                                    let assignees_str = {
                                        let assignee_names = issue
                                            .assignees
                                            .as_ref()
                                            .and_then(|e| e.edges.as_ref())
                                            .map_or(Vec::new(), |assignee_edges| {
                                                assignee_edges
                                                    .iter()
                                                    .filter_map(|edge| {
                                                        edge.as_ref().and_then(|actual_edge| {
                                                            actual_edge.node.as_ref().and_then(
                                                                |user| {
                                                                    user.login.as_ref().map(
                                                                        |login_str| {
                                                                            login_str.as_str()
                                                                        },
                                                                    )
                                                                },
                                                            )
                                                        })
                                                    })
                                                    .collect::<Vec<&str>>()
                                            });

                                        if !assignee_names.is_empty() {
                                            format!("Assignees: {},", assignee_names.join(", "))
                                        } else {
                                            String::new()
                                        }
                                    };

                                    let state_str = match &issue.state {
                                        Some(s) => format!("State: {},", s),
                                        None => String::new(),
                                    };

                                    let body_str = match &issue.body {
                                        Some(body_text) if body_text.len() > 180 => {
                                            let truncated_body = body_text
                                                .chars()
                                                .take(100)
                                                .chain(
                                                    body_text
                                                        .chars()
                                                        .skip(body_text.chars().count() - 80),
                                                )
                                                .collect::<String>();

                                            format!("Body: {}", truncated_body)
                                        }
                                        Some(body_text) => format!("Body: {},", body_text),
                                        None => String::new(),
                                    };

                                    let assoc_str = match &issue.authorAssociation {
                                        Some(association) => {
                                            format!("Author Association: {}", association)
                                        }
                                        None => String::new(),
                                    };

                                    let temp = format!(
                                                    "{title_str} {url_str} Created At: {date} {author_str} {assignees_str}  {state_str} {body_str} {assoc_str}");

                                    out.push_str(&temp);
                                    out.push_str("\n");
                                } else {
                                    continue;
                                }
                            }
                        }

                        if let Some(page_info) = &search.pageInfo {
                            if let Some(has_next_page) = page_info.hasNextPage {
                                if has_next_page {
                                    match &page_info.endCursor {
                                        Some(end_cursor) => {
                                            cursor = Some(end_cursor.clone());
                                            log::info!(
                                                "Fetched a page, moving to next page with cursor: {}",
                                                end_cursor
                                            );
                                            continue;
                                        }
                                        None => {
                                            log::error!("Warning: hasNextPage is true, but endCursor is None. This might result in missing data.");
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    break;
                }
            },
        }
    }

    Some(out)
}

pub async fn search_repository(search_query: &str) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct Payload {
        data: Option<Data>,
    }

    #[derive(Debug, Deserialize)]
    struct Data {
        search: Option<Search>,
    }

    #[derive(Debug, Deserialize)]
    struct Search {
        edges: Option<Vec<Option<Edge>>>,
        pageInfo: Option<PageInfo>,
    }

    #[derive(Debug, Deserialize)]
    struct Edge {
        node: Option<Node>,
    }

    #[derive(Debug, Deserialize)]
    struct Node {
        name: Option<String>,
        description: Option<String>,
        url: Option<String>,
        createdAt: Option<DateTime<Utc>>,
        stargazers: Option<Stargazers>,
        forkCount: Option<u32>,
    }

    #[derive(Debug, Deserialize)]
    struct Stargazers {
        totalCount: Option<u32>,
    }

    #[derive(Debug, Deserialize)]
    struct PageInfo {
        endCursor: Option<String>,
        hasNextPage: Option<bool>,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let base_url = "https://api.github.com/graphql";
    let mut out = String::from("REPOSITORY \n");

    let mut cursor: Option<String> = None;

    loop {
        let query = format!(
            r#"
                query {{
                    search(query: "{search_query}", type: REPOSITORY, first: 100{after}) {{
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
                        pageInfo {{
                            endCursor
                            hasNextPage
                        }}
                    }}
                }}
            "#,
            search_query = search_query,
            after = cursor
                .as_ref()
                .map_or(String::new(), |c| format!(r#", after: "{}""#, c))
        );

        match github_http_post(&github_token, base_url, &query).await {
            None => {
                log::error!(
                    "Failed to send the request to get RepositoryRoot: {}",
                    base_url
                );
                return None;
            }
            Some(response) => match serde_json::from_slice::<Payload>(response.as_slice()) {
                Err(e) => {
                    log::error!("Failed to parse the response for RepositoryRoot: {}", e);
                    return None;
                }
                Ok(payload) => {
                    if let Some(data) = &payload.data {
                        if let Some(search) = &data.search {
                            if let Some(edges) = &search.edges {
                                for edge_option in edges {
                                    if let Some(edge) = edge_option {
                                        if let Some(repo) = &edge.node {
                                            let date_str = match &repo.createdAt {
                                                Some(date) => date.date_naive().to_string(),
                                                None => continue,
                                            };

                                            let name_str = match &repo.name {
                                                Some(name) => format!("Name: {name},"),
                                                None => String::new(),
                                            };

                                            let desc_str = match &repo.description {
                                                Some(desc) if desc.len() > 300 => {
                                                    let truncated_desc = desc
                                                        .chars()
                                                        .take(180)
                                                        .chain(
                                                            desc.chars()
                                                                .skip(desc.chars().count() - 120),
                                                        )
                                                        .collect::<String>();

                                                    format!("Description: {truncated_desc}")
                                                }
                                                Some(desc) => format!("Description: {desc},"),
                                                None => String::new(),
                                            };

                                            let url_str = match &repo.url {
                                                Some(url) => format!("Url: {url}"),
                                                None => String::new(),
                                            };

                                            let stars_str = match &repo.stargazers {
                                                Some(sg) => format!(
                                                    "Stars: {},",
                                                    sg.totalCount.unwrap_or(0)
                                                ),
                                                None => String::new(),
                                            };

                                            let forks_str = match &repo.forkCount {
                                                Some(forkCount) => format!("Forks: {forkCount}"),
                                                None => String::new(),
                                            };

                                            out.push_str(&format!("{name_str} {desc_str} {url_str} Created At: {date_str} {stars_str} {forks_str}\n"));
                                        }
                                    }
                                }
                            }
                            if let Some(page_info) = &search.pageInfo {
                                if page_info.hasNextPage.unwrap_or(false) {
                                    cursor = page_info.endCursor.clone();
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
            },
        };
    }

    Some(out)
}

pub async fn search_discussion(search_query: &str) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct DiscussionRoot {
        data: Option<DiscussionData>,
    }

    #[derive(Debug, Deserialize)]
    struct DiscussionData {
        search: Option<DiscussionSearch>,
    }

    #[derive(Debug, Deserialize)]
    struct DiscussionSearch {
        edges: Option<Vec<Option<DiscussionNode>>>,
        pageInfo: Option<PageInfo>,
    }

    #[derive(Debug, Deserialize)]
    struct PageInfo {
        hasNextPage: Option<bool>,
        endCursor: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct DiscussionNode {
        node: Option<Discussion>,
    }

    #[derive(Debug, Deserialize)]
    struct Discussion {
        title: Option<String>,
        url: Option<String>,
        createdAt: Option<DateTime<Utc>>,
        upvoteCount: Option<i32>,
    }
    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let base_url = "https://api.github.com/graphql";
    let mut out = String::from("DISCUSSION: \n");

    let mut cursor: Option<String> = None;

    loop {
        let query = format!(
            r#"
            query {{
                search(query: "{search_query}", type: DISCUSSION, first: 100{after}) {{
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
                    pageInfo {{
                        hasNextPage
                        endCursor
                    }}
                }}
            }}
            "#,
            search_query = search_query,
            after = cursor
                .as_ref()
                .map_or(String::new(), |c| format!(r#", after: "{}""#, c))
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
                    if let Some(search) = &results.data?.search {
                        if let Some(edges) = &search.edges {
                            for edge_option in edges {
                                if let Some(discussion_node) = edge_option {
                                    if let Some(discussion) = &discussion_node.node {
                                        let date = match &discussion.createdAt {
                                            Some(date) => date.date_naive().to_string(),
                                            None => continue,
                                        };

                                        let title_str = match &discussion.title {
                                            Some(title) => format!("Title: {},", title),
                                            None => String::new(),
                                        };

                                        let url_str = match &discussion.url {
                                            Some(u) => format!("Url: {}", u),
                                            None => String::new(),
                                        };

                                        let upvotes_str = match discussion.upvoteCount {
                                            Some(count) => format!("Upvotes: {}", count),
                                            None => String::new(),
                                        };

                                        out.push_str(&format!(
                                            "{title_str} {url_str} Created At: {date} {upvotes_str}\n"
                                        ));
                                    }
                                }
                            }
                        }
                        if let Some(page_info) = &search.pageInfo {
                            if page_info.hasNextPage.unwrap_or(false) {
                                cursor = page_info.endCursor.clone();
                            } else {
                                break;
                            }
                        }
                    }
                }
            },
        };
    }

    Some(out)
}

pub async fn search_users(search_query: &str) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct User {
        name: Option<String>,
        login: Option<String>,
        url: Option<String>,
        twitterUsername: Option<String>,
        bio: Option<String>,
        company: Option<String>,
        location: Option<String>,
        createdAt: Option<DateTime<Utc>>,
        email: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct UserNode {
        node: Option<User>,
    }

    #[derive(Debug, Deserialize)]
    struct UserEdge {
        edges: Option<Vec<Option<UserNode>>>,
    }

    #[derive(Debug, Deserialize)]
    struct UserSearch {
        search: Option<UserEdge>,
    }

    #[derive(Debug, Deserialize)]
    struct UserRoot {
        data: Option<UserSearch>,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let base_url = "https://api.github.com/graphql";
    let mut out = String::from("USERS: \n");

    let query = format!(
        r#"
        query {{
            search(query: "{search_query}", type: USER, first: 100) {{
                edges {{
                    node {{
                        ... on User {{
                            name
                            login
                            url
                            twitterUsername
                            bio
                            company
                            location
                            createdAt
                            email
                        }}
                    }}
                }}
            }}
        }}
        "#,
        search_query = search_query,
    );

    match github_http_post(&github_token, base_url, &query).await {
        None => {
            log::error!("Failed to send the request to get UserRoot: {}", base_url);
            return None;
        }
        Some(res) => match serde_json::from_slice::<UserRoot>(res.as_slice()) {
            Err(e) => {
                log::error!("Failed to parse the response for UserRoot: {}", e);
                return None;
            }
            Ok(results) => {
                if let Some(search) = &results.data {
                    if let Some(edges) = &search.search {
                        for edge_option in edges.edges.as_ref().unwrap_or(&vec![]) {
                            if let Some(edge) = edge_option {
                                if let Some(user) = &edge.node {
                                    let login_str = match &user.login {
                                        Some(login) => format!("Login: {},", login),
                                        None => continue,
                                    };
                                    let name_str = match &user.name {
                                        Some(name) => format!("Name: {},", name),
                                        None => String::new(),
                                    };

                                    let url_str = match &user.url {
                                        Some(url) => format!("Url: {},", url),
                                        None => String::new(),
                                    };

                                    let twitter_str = match &user.twitterUsername {
                                        Some(twitter) => format!("Twitter: {},", twitter),
                                        None => String::new(),
                                    };

                                    let bio_str = match &user.bio {
                                        Some(bio) => format!("Bio: {},", bio),
                                        None => String::new(),
                                    };

                                    let company_str = match &user.company {
                                        Some(company) => format!("Company: {},", company),
                                        None => String::new(),
                                    };

                                    let location_str = match &user.location {
                                        Some(location) => format!("Location: {},", location),
                                        None => String::new(),
                                    };

                                    let date_str = match &user.createdAt {
                                        Some(date) => {
                                            format!(
                                                "Created At: {},",
                                                date.date_naive().to_string()
                                            )
                                        }
                                        None => String::new(),
                                    };

                                    let email_str = match &user.email {
                                        Some(email) => format!("Email: {}", email),
                                        None => String::new(),
                                    };

                                    out.push_str(&format!(
                                        "{name_str} {login_str} {url_str} {twitter_str} {bio_str} {company_str} {location_str} {date_str} {email_str}\n"
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        },
    };

    Some(out)
}
