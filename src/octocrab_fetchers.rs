// use crate::utils::*;
// use chrono::{DateTime, Utc};
// use dotenv::dotenv;
// use flowsnet_platform_sdk::logger;
// use github_flows::{
//     get_octo,
//     octocrab::{
//         models::{issues::Issue, Repository, User},
//         params::{
//             repos::{Sort, Type},
//             Direction,
//         },
//         Error as OctoError, Page, Result as OctoResult,
//     },
//     GithubLogin,
// };
// use log::{self, debug};
// use serde::{Deserialize, Serialize};
// use serde_json;
// use std::env;
// use store_flows::{get, set};

// pub async fn get_user_repos_octo_alt(username: &str, language: &str) -> Option<String> {
//     let octocrab = get_octo(&GithubLogin::Default);

//     let mut repositories = Vec::new();

//     let mut page_number = 0u8;
//     loop {
//         let page = octocrab
//             .search()
//             .repositories(&format!("user:{} language:{}", username, language))
//             .sort("stars")
//             .order("desc")
//             .per_page(100)
//             .page(page_number)
//             .send()
//             .await;

//         match page {
//             Ok(p) => {
//                 if p.items.is_empty() {
//                     break;
//                 }
//                 repositories.extend(p.items.into_iter().map(|repo| repo.name));
//                 page_number += 1;
//             }
//             Err(e) => {
//                 log::error!("Failed to get repositories for user {}: {}", username, e);
//                 break;
//             }
//         }
//     }
//     Some(repositories.join(", "))
// }
// pub async fn get_org_repos_octo(org: &str) -> Option<String> {
//     let octocrab = get_octo(&GithubLogin::Default);

//     let page: OctoResult<Page<Repository>> = octocrab
//         .orgs(org)
//         .list_repos()
//         // Optional Parameters
//         .repo_type(Type::Sources)
//         .sort(Sort::Pushed)
//         .direction(Direction::Descending)
//         .per_page(25)
//         .page(5u32)
//         // Send the request.
//         .send()
//         .await;

//     match page {
//         Ok(p) => Some(format!("{:?}", p)),
//         Err(_e) => {
//             log::error!("Repo for org: {org} not found: {}", _e);
//             None
//         }
//     }
// }

// pub async fn get_user_profile(user: &str) -> Option<String> {
//     let octocrab = get_octo(&GithubLogin::Default);
//     let user_route = format!("/users/{user}");
//     let user: OctoResult<User> = octocrab.get(&user_route, None::<&()>).await;

//     match user {
//         Ok(u) => Some(format!("{:?}", u)),
//         Err(_e) => {
//             log::error!("Github user not found: {}", _e);
//             None
//         }
//     }
// }

// pub async fn get_user_repos_octo(user_name: &str, language: &str) -> Option<String> {
//     #[derive(Debug, Deserialize)]
//     struct Root {
//         data: Data,
//     }

//     #[derive(Debug, Deserialize)]
//     struct Data {
//         search: Search,
//     }

//     #[derive(Debug, Deserialize)]
//     struct Search {
//         nodes: Vec<Node>,
//     }

//     #[derive(Debug, Deserialize)]
//     struct Node {
//         name: String,
//         defaultBranchRef: BranchRef,
//         stargazers: Stargazers,
//     }

//     #[derive(Debug, Deserialize)]
//     struct BranchRef {
//         target: Target,
//     }

//     #[derive(Debug, Deserialize)]
//     struct Target {
//         history: History,
//     }

//     #[derive(Debug, Deserialize)]
//     struct History {
//         #[serde(rename = "totalCount")]
total_count: i32,
//     }

//     #[derive(Debug, Deserialize)]
//     struct Stargazers {
//         #[serde(rename = "totalCount")]
total_count: i32,
//     }
//     let octocrab = get_octo(&GithubLogin::Default);
//     let query = format!(
//         r#"
//     query {{
//         search(query: "user:{} language:{}", type: REPOSITORY, first: 100) {{
//             nodes {{
//                 ... on Repository {{
//                     name
//                     defaultBranchRef {{
//                         target {{
//                             ... on Commit {{
//                                 history(first: 0) {{
//                                     totalCount
//                                 }}
//                             }}
//                         }}
//                     }}
//                     stargazers {{
//                         totalCount
//                     }}
//                 }}
//             }}
//         }}
//     }}
//     "#,
//         user_name, language
//     );

//     let res: Result<Root, OctoError> = octocrab
//         .graphql(&serde_json::json! ({
//             "query": query
//         }))
//         .await;

//     let mut out = String::new();
//     match res {
//         Err(_e) => log::error!("Failed to send the request to {}", _e.to_string()),
//         Ok(response) => {
//             let mut repos_sorted: Vec<&Node> = response.data.search.nodes.iter().collect();
//             repos_sorted.sort_by(|a, b| b.stargazers.total_count.cmp(&a.stargazers.total_count));

//             for repo in repos_sorted {
//                 let temp = format!(
//                     "Repo: {}, Stars: {}, Commits: {}",
//                     repo.name,
//                     repo.stargazers.total_count,
//                     repo.default_branch_ref.target.history.total_count
//                 );
//                 out.push_str(&temp);
//             }

//             log::error!("Found {} repositories", response.data.search.nodes.len());
//         }
//     };
//     Some(out)
// }
