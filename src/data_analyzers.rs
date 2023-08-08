use crate::octocrab_compat::{Comment, Issue};
use crate::utils::*;
use chrono::{DateTime, Duration, Utc};
use log;
use serde::{Deserialize, Serialize};
use serde_json;
use std::env;

pub async fn process_commits_in_range(
    owner: &str,
    repo: &str,
    user_name: Option<&str>,
    range: u16,
) -> Option<(String, usize)> {
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
        commit: CommitDetails,
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct CommitDetails {
        author: CommitUserDetails,
        // committer: CommitUserDetails,
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct CommitUserDetails {
        date: Option<DateTime<Utc>>,
    }

    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());

    let author_str = match user_name {
        Some(user_name) => format!("?author={}", user_name),
        None => "".to_string(),
    };

    let commits_url_str =
        format!("https://api.github.com/repos/{owner}/{repo}/commits{author_str}",);

    let mut commits_summaries = String::new();
    let now = Utc::now();
    let n_days_ago = (now - Duration::days(range as i64)).date_naive();
    let mut commits_count = 0;
    match github_http_fetch(&github_token, &commits_url_str).await {
        None => log::error!("Error fetching Page of commits"),
        Some(res) => match serde_json::from_slice::<Vec<GithubCommit>>(res.as_slice()) {
            Err(e) => log::error!("Error parsing commits object: {:?}", e),
            Ok(commits_obj) => {
                let recent_commits: Vec<_> = commits_obj
                    .into_iter()
                    .filter(|commit| {
                        if let Some(commit_date) = &commit.commit.author.date {
                            let commit_naive_date = commit_date.date_naive();
                            commit_naive_date > n_days_ago
                        } else {
                            false
                        }
                    })
                    .collect();

                for commit in &recent_commits {
                    let user_name = &commit.author.login;
                    match analyze_commit(owner, repo, user_name, &commit.sha).await {
                        Some(summary) => {
                            commits_count += 1;
                            commits_summaries.push_str(&summary);
                            commits_summaries.push('\n');
                            if commits_summaries.len() > 45_000 {
                                break;
                            }
                        }
                        None => {
                            log::error!(
                                "Error analyzing commit {:?} for user {}",
                                commit.sha,
                                user_name
                            )
                        }
                    }
                }
            }
        },
    }

    Some((commits_summaries, commits_count))
}

pub async fn analyze_commit(owner: &str, repo: &str, user_name: &str, sha: &str) -> Option<String> {
    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());

    let commit_patch_str = format!("https://github.com/{owner}/{repo}/commit/{sha}.patch");
    match github_http_fetch(&github_token, &commit_patch_str).await {
        Some(res) => {
            let text = String::from_utf8_lossy(res.as_slice()).to_string();

            let sys_prompt_1 = &format!("You are provided with a commit patch by the user {user_name} on the {repo} project. Your task is to parse this data, focusing on the following sections: the Date Line, Subject Line, Diff Files, Diff Changes, Sign-off Line, and the File Changes Summary. Extract key elements such as the date of the commit (in 'yyyy/mm/dd' format), a summary of changes, and the types of files affected, prioritizing code files, scripts, then documentation. Be particularly careful to distinguish between changes made to core code files and modifications made to documentation files, even if they contain technical content. Compile a list of the extracted key elements.");

            let usr_prompt_1 = &format!("Based on the provided commit patch: {text}, extract and present the following key elements: the date of the commit (formatted as 'yyyy/mm/dd'), a high-level summary of the changes made, and the types of files affected. Prioritize data on changes to code files first, then scripts, and lastly documentation. Pay attention to the file types and ensure the distinction between documentation changes and core code changes, even when the documentation contains highly technical language. Please compile your findings into a list, with each key element represented as a separate item.");

            let usr_prompt_2 = &format!("Using the key elements you extracted from the commit patch, provide a summary of the user's contributions to the project. Include the date of the commit, the types of files affected, and the overall changes made. When describing the affected files, make sure to differentiate between changes to core code files, scripts, and documentation files. Present your summary in this format: 'On (date in 'yyyy/mm/dd' format), (summary of changes). (overall impact of changes).' Please ensure your answer stayed below 128 tokens.");

            let sha_serial = sha.chars().take(5).collect::<String>();
            chain_of_chat(
                sys_prompt_1,
                usr_prompt_1,
                &format!("commit-{sha_serial}"),
                256,
                usr_prompt_2,
                128,
                &format!("analyze_commit-{sha_serial}"),
            )
            .await
        }
        None => None,
    }
}

pub async fn correlate_commits_issues(
    _commits_summary: &str,
    _issues_summary: &str,
) -> Option<String> {
    let (commits_summary, issues_summary) =
        squeeze_fit_commits_issues(_commits_summary, _issues_summary, 0.6);

    let sys_prompt_1 = &format!("Your task is to identify the 1-3 most impactful contributions by a specific user, based on the given commit logs and issue records. Pay close attention to any sequential relationships between issues and commits, and consider how they reflect the user's growth and evolution within the project. Use this data to evaluate the user's overall influence on the project's development. Provide a concise summary in bullet-point format.");

    let usr_prompt_1 = &format!("Given the commit logs: {commits_summary} and issue records: {issues_summary}, identify the most significant contributions made by the user. Look for patterns and sequences of events that indicate the user's growth and how they approached problem-solving. Consider major code changes, and initiatives that had substantial impact on the project. Additionally, note any instances where the resolution of an issue led to a specific commit.");

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
pub async fn correlate_commits_issues_discussions(
    _commits_summary: &str,
    _issues_summary: &str,
    _discussions: &str,
) -> Option<String> {
    // Adjusting the squeeze function to account for discussions
    let (commits_summary, issues_summary) =
        squeeze_fit_commits_issues(_commits_summary, _issues_summary, 0.6);

    // let sys_prompt_1 = &format!("Your task is to identify the 1-3 most impactful contributions by a specific user, based on the given commit logs, issue records, and discussion threads. Pay close attention to any sequential relationships among them. Consider how discussions lead to issue creations or commits and vice-versa, reflecting the user's growth and evolution within the project. Use this data to evaluate the user's overall influence on the project's development. Provide a concise summary in bullet-point format.");

    // let usr_prompt_1 = &format!("Given the commit logs: {commits_summary}, issue records: {issues_summary}, and discussion threads: {_discussions}, identify the most significant contributions made by the user. Look for patterns and sequences of events that indicate the user's growth and how they approached problem-solving. Consider major discussions leading to code changes, or the resolution of issues that led to specific commits or spawned further discussions.");

    // let usr_prompt_2 = &format!("Based on the contributions identified, create a concise bullet-point summary. Highlight top contributor's key contributions and their influence on the project using commits, issues, and discussions. Pay attention to their growth over time, and how their responses and engagements evolved. Make sure to reference any interconnected events among the three. Avoid replicating phrases from the source data and focus on providing a unique and insightful narrative. Please ensure your answer stays below 256 tokens.");
    let sys_prompt_1 = &format!("Analyze the given commit logs, issue records, and discussion threads to identify and quantify the impactful contributions of each individual member during the week. Focus on specific changes, improvements, or resolutions that each member contributed. Provide a bullet-point analysis for each member, emphasizing their distinct contributions and measurable impact on the project.");

    let usr_prompt_1 = &format!("Given the commit logs: {commits_summary}, issue records: {issues_summary}, and discussion threads: {_discussions}, identify the contributions of each member during the week. List down the specific tasks, enhancements, or resolutions made by each individual. Ensure the focus is on concrete and impactful contributions that each member brought to the project.");

    let usr_prompt_2 = &format!("Now, synthesize the individual analyses into a cohesive summary. Describe how each member's contributions this week fit into the overall progression and objectives of the project. Focus on collaboration, interplay between different contributions, and the collective impact of the team. Mention any overarching themes or patterns observed during the week. Ensure the narrative reflects both individual efforts and their combined influence on the project's advancement. Limit your answer to 256 tokens.");

    chain_of_chat(
        sys_prompt_1,
        usr_prompt_1,
        "correlate-99",
        512,
        usr_prompt_2,
        256,
        "correlate_commits_issues_discussions",
    )
    .await
}

pub async fn correlate_user_and_home_project(
    home_repo_data: &str,
    user_profile: &str,
    issues_data: &str,
    repos_data: &str,
    discussion_data: &str,
) -> Option<String> {
    let home_repo_data = home_repo_data.chars().take(6000).collect::<String>();
    let user_profile = user_profile.chars().take(4000).collect::<String>();
    let issues_data = issues_data.chars().take(9000).collect::<String>();
    let repos_data = repos_data.chars().take(6000).collect::<String>();
    let discussion_data = discussion_data.chars().take(4000).collect::<String>();

    let sys_prompt_1 = &format!("First, let's analyze and understand the provided Github data in a step-by-step manner. Begin by evaluating the user's activity based on their most active repositories, languages used, issues they're involved in, and discussions they've participated in. Concurrently, grasp the characteristics and requirements of the home project. Your aim is to identify overlaps or connections between the user's skills or activities and the home project's needs.");

    let usr_prompt_1 = &format!("Using a structured approach, analyze the given data: User Profile: {} Active Repositories: {} Issues Involved: {} Discussions Participated: {} Home project's characteristics: {} Identify patterns in the user's activity and spot potential synergies with the home project. Pay special attention to the programming languages they use, especially if they align with the home project's requirements. Derive insights from their interactions and the data provided.", user_profile, repos_data, issues_data, discussion_data, home_repo_data);

    let usr_prompt_2 = &format!("Now, using the insights from your step-by-step analysis, craft a concise bullet-point summary that underscores: - The user's main areas of expertise and interest. - The relevance of their preferred languages or technologies to the home project. - Their potential contributions to the home project, based on their skills and interactions. Ensure the summary is clear, insightful, and remains under 256 tokens. Emphasize any evident alignments between the user's skills and the project's needs.");
    chain_of_chat(
        sys_prompt_1,
        usr_prompt_1,
        "correlate-user-home",
        512,
        usr_prompt_2,
        256,
        "correlate-user-home-summary",
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

/* pub async fn analyze_discussion(owner: &str, repo: &str, user: &str, discussion: Discussion) -> Option<String> {
    let github_token = env::var("github_token").unwrap_or("fake-token".to_string());

    let discussion_creator_name = discussion.user.login;
    let discussion_number = discussion.number;
    let discussion_title = discussion.title;
    let discussion_body = match discussion.body {
        Some(body) => squeeze_fit_comment_texts(&body, "```", 500, 0.6),
        None => "".to_string(),
    };
    let discussion_date = discussion.created_at.date_naive().to_string();
    let html_url = discussion.html_url.to_string();

    let mut all_text_from_discussion = format!("User '{discussion_creator_name}', has started a discussion titled '{discussion_title}', with the opening post: '{discussion_body}'.");

    let url_str = format!(
        "https://api.github.com/repos/{owner}/{repo}/discussions/{discussion_number}/comments?per_page=100",
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
                    all_text_from_discussion.push_str(&commenter_input);

                    if all_text_from_discussion.len() > 45_000 {
                        break;
                    }
                }
            }
        },
        None => {}
    };

    let sys_prompt_1 = &format!("Given the information that user '{discussion_creator_name}' opened a discussion titled '{discussion_title}', your task is to analyze the content of the discussion posts. Extract key points, topics discussed, any problems or questions raised, significant contributions from participants, and any identified conclusions or action items.");

    let usr_prompt_1 = &format!("Based on the GitHub discussion posts: {all_text_from_discussion}, please extract: Main topics or points discussed. Any questions or problems raised. Key contributions or points of view shared by participants. Significant contributions by user '{user}'. Identified conclusions or action items. The role and contribution of the user '{user}' in the discussion.");

    let usr_prompt_2 = &format!("Provide a concise summary emphasizing the overarching contribution made by '{user}' in the discussion and the core themes addressed, ensuring your response is under 128 tokens.");

    match chain_of_chat(
        sys_prompt_1,
        usr_prompt_1,
        &format!("discussion_{discussion_number}"),
        256,
        usr_prompt_2,
        128,
        &format!("Error generating discussion summary #{discussion_number}"),
    )
    .await
    {
        Some(discussion_summary) => {
            let mut out = html_url.to_string();
            out.push(' ');
            out.push_str(&discussion_summary);
            return Some(out);
        }
        None => {}
    }

    None
} */
