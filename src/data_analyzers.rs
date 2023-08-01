use crate::utils::*;
use dotenv::dotenv;
use flowsnet_platform_sdk::logger;
use github_flows::octocrab::models::issues::{Comment, Issue};
use log;
use serde::{Deserialize, Serialize};
use serde_json;
use std::env;


pub async fn analyze_commits(owner: &str, repo: &str, user_name: &str) -> Option<String> {
    #[derive(Debug, Deserialize, Serialize)]
    struct User {
        login: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct GithubCommit {
        sha: String,
        html_url: String,
        author: User,
        committer: User,
    }
    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());
    let user_commits_repo_str =
        format!("https://api.github.com/repos/{owner}/{repo}/commits?author={user_name}");
    let mut commits_summaries = String::new();

    match github_http_fetch(&github_token, &user_commits_repo_str).await {
        None => log::error!("Error fetching Page of commits"),
        Some(res) => match serde_json::from_slice::<Vec<GithubCommit>>(res.as_slice()) {
            Err(_e) => log::error!("Error parsing commits object: {:?}", _e),
            Ok(commits_obj) => {
                for sha in commits_obj.into_iter().map(|commit| commit.sha) {
                    let commit_patch_str =
                        format!("https://github.com/{owner}/{repo}/commit/{sha}.patch");
                    match github_http_fetch(&github_token, &commit_patch_str).await {
                        Some(res) => {
                            let text = String::from_utf8_lossy(res.as_slice()).to_string();

                            let sys_prompt_1 = &format!("You are provided with a commit patch by the user {user_name} on the {repo} project. Your task is to parse this data, focusing on the following sections: the Date Line, Subject Line, Diff Files, Diff Changes, Sign-off Line, and the File Changes Summary. Extract key elements such as the date of the commit (in 'yyyy/mm/dd' format), a summary of changes, and the types of files affected, prioritizing code files, scripts, then documentation. Be particularly careful to distinguish between changes made to core code files and modifications made to documentation files, even if they contain technical content. Compile a list of the extracted key elements.");

                            let usr_prompt_1 = &format!("Based on the provided commit patch: {text}, extract and present the following key elements: the date of the commit (formatted as 'yyyy/mm/dd'), a high-level summary of the changes made, and the types of files affected. Prioritize data on changes to code files first, then scripts, and lastly documentation. Pay attention to the file types and ensure the distinction between documentation changes and core code changes, even when the documentation contains highly technical language. Please compile your findings into a list, with each key element represented as a separate item.");

                            let usr_prompt_2 = &format!("Using the key elements you extracted from the commit patch, provide a summary of the user's contributions to the project. Include the date of the commit, the types of files affected, and the overall changes made. When describing the affected files, make sure to differentiate between changes to core code files, scripts, and documentation files. Present your summary in this format: 'On (date in 'yyyy/mm/dd' format), (summary of changes). (overall impact of changes).' Please ensure your answer stayed below 128 tokens.");

                            let sha_serial = sha.chars().take(5).collect::<String>();
                            match chain_of_chat(
                                sys_prompt_1,
                                usr_prompt_1,
                                &format!("commit-{sha_serial}"),
                                256,
                                usr_prompt_2,
                                128,
                                &format!("analyze_commits-{sha_serial}"),
                            )
                            .await
                            {
                                Some(res) => {
                                    commits_summaries.push_str(&res);
                                    commits_summaries.push('\n');
                                    if commits_summaries.len() > 45_000 {
                                        break;
                                    }
                                }
                                None => continue,
                            }
                        }
                        None => continue,
                    };
                }
            }
        },
    }

    Some(commits_summaries)
}

pub async fn correlate_commits_issues(
    _commits_summary: &str,
    _issues_summary: &str,
) -> Option<String> {
    let (commits_summary, issues_summary) =
        squeeze_fit_commits_issues(_commits_summary, _issues_summary, 0.6);

    // let sys_prompt_1 = &format!("Your task is to examine and correlate both commit logs and issue records for a specific user within a GitHub repository. Despite potential limitations in the data, such as insufficient information or difficulties in finding correlations, focus on identifying the user's top 1-3 significant contributions to the project. Consider all aspects of their contributions, from the codebase to project documentation, and describe their evolution over time. Assess the overall impact of these contributions to the project's development. Create a unique, detailed summary that highlights the scope and significance of the user's contributions, avoiding verbatim repetition from the source data. If correlations between commit logs and issue records are limited, prioritize identifying the user's top contributions. Present your summary in a clear, bullet-point format.");
    let sys_prompt_1 = &format!("Your task is to identify the 1-3 most impactful contributions by a specific user, based on the given commit logs and issue records. Pay close attention to any sequential relationships between issues and commits, and consider how they reflect the user's growth and evolution within the project. Use this data to evaluate the user's overall influence on the project's development. Provide a concise summary in bullet-point format.");

    // let usr_prompt_1 = &format!("Given the commit logs: {commits_summary} and issue records: {issues_summary}, analyze and identify the top 1-3 significant contributions made by the user to the project. Your task is to recognize the key areas of impact, be it in the codebase, project documentation, or other aspects, even in the presence of insufficient data or lack of direct correlations. Create a list of these significant contributions without directly replicating phrases from the source data. This list will be used in the next step to construct a detailed narrative of the user's journey in the project.");
    let usr_prompt_1 = &format!("Given the commit logs: {commits_summary} and issue records: {issues_summary}, identify the most significant contributions made by the user. Look for patterns and sequences of events that indicate the user's growth and how they approached problem-solving. Consider major code changes, and initiatives that had substantial impact on the project. Additionally, note any instances where the resolution of an issue led to a specific commit.");

    // let usr_prompt_2 = &format!("Using the list of significant contributions identified in the previous step, create a detailed narrative that depicts the user's journey and evolution in the project. Describe the progression of these contributions over time, from their inception to their current status. Highlight the overall impact and significance of these contributions within the project's development. Your narrative should be unique and insightful, capturing the user's influence on the project. Present your findings in a clear, concise, and bullet-point format.");
    let usr_prompt_2 = &format!("Based on the contributions identified, create a concise bullet-point summary. Highlight the user's key contributions and their influence on the project. Pay attention to their growth over time, and how their responses to issues evolved. Make sure to reference any interconnected events between issues and commits. Avoid replicating phrases from the source data and focus on providing a unique and insightful narrative. Please ensure your answer stayed below 256 tokens.");

    chain_of_chat(
        sys_prompt_1,
        usr_prompt_1,
        "correlate-99",
        512,
        usr_prompt_2,
        256,
        "correlate_commits_issues",
    )
    .await
}

pub async fn analyze_issue(owner: &str, repo: &str, user: &str, issue: Issue) -> Option<String> {
    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());

    let issue_creator_name = issue.user.login;
    let issue_number = issue.number;
    let issue_title = issue.title;
    let issue_body = match issue.body {
        Some(body) => squeeze_fit_comment_texts(&body, "```", 500, 0.6),
        None => "".to_string(),
    };
    let issue_date = issue.created_at.date_naive().to_string();
    let html_url = issue.html_url.to_string();

    let labels = issue
        .labels
        .into_iter()
        .map(|lab| lab.name)
        .collect::<Vec<String>>()
        .join(", ");

    let mut all_text_from_issue = format!("User '{issue_creator_name}', has submitted an issue titled '{issue_title}', labeled as '{labels}', with the following post: '{issue_body}'.");

    let url_str = format!(
        "https://api.github.com/repos/{owner}/{repo}/issues/{issue_number}/comments?per_page=100",
    );

    match github_http_fetch(&github_token, &url_str).await {
        Some(res) => match serde_json::from_slice::<Vec<Comment>>(res.as_slice()) {
            Err(_e) => log::error!("Error parsing Vec<Comment>: {:?}", _e),
            Ok(comments_obj) => {
                for comment in comments_obj {
                    let comment_body = match comment.body {
                        Some(body) => squeeze_fit_comment_texts(&body, "```", 500, 0.6),
                        None => "".to_string(),
                    };
                    let commenter = comment.user.login;
                    let commenter_input = format!("{commenter} commented: {comment_body}");
                    all_text_from_issue.push_str(&commenter_input);

                    if all_text_from_issue.len() > 45_000 {
                        break;
                    }
                }
            }
        },
        None => {}
    };

    let sys_prompt_1 = &format!("Given the information that user '{issue_creator_name}' opened an issue titled '{issue_title}', labelled as '{labels}', your task is to analyze the content of the issue posts. Extract key details including the main problem or question raised, the environment in which the issue occurred, any steps taken by the user to address the problem, relevant discussions, and any identified solutions or pending tasks.");
    let usr_prompt_1 = &format!("Based on the GitHub issue posts: {all_text_from_issue}, please list the following key details: The main problem or question raised in the issue. The environment or conditions in which the issue occurred (e.g., hardware, OS). Any steps or actions taken by the user '{user}' or others to address the issue. Key discussions or points of view shared by participants in the issue thread. Any solutions identified, or pending tasks if the issue hasn't been resolved. The role and contribution of the user '{user}' in the issue.");
    let usr_prompt_2 = &format!("Provide a brief summary highlighting the core problem and emphasize the overarching contribution made by '{user}' to the resolution of this issue, ensuring your response stays under 128 tokens.");

    match chain_of_chat(
        sys_prompt_1,
        usr_prompt_1,
        &format!("issue_{issue_number}"),
        256,
        usr_prompt_2,
        128,
        &format!("Error generatng issue summary #{issue_number}"),
    )
    .await
    {
        Some(issue_summary) => {
            let mut out = html_url.to_string();
            out.push(' ');
            out.push_str(&issue_summary);
            return Some(out);
        }
        None => {}
    }

    None
}
