use crate::github_data_fetchers::*;
use crate::octocrab_compat::{Comment, Issue};
use crate::utils::*;
use chrono::{DateTime, Utc};
use log;
use openai_flows::{
    self,
    chat::{self, ChatOptions},
    OpenAIFlows,
};
use serde::Deserialize;

pub async fn is_valid_owner_repo_integrated(
    github_token: &str,
    owner: &str,
    repo: &str,
) -> Option<GitMemory> {
    #[derive(Deserialize)]
    struct CommunityProfile {
        health_percentage: u16,
        description: Option<String>,
        readme: Option<String>,
        updated_at: Option<DateTime<Utc>>,
    }
    let openai = OpenAIFlows::new();

    let community_profile_url = format!(
        "https://api.github.com/repos/{}/{}/community/profile",
        owner, repo
    );

    let mut description = String::new();
    let mut date = Utc::now().date_naive();
    match github_http_fetch(&github_token, &community_profile_url).await {
        Some(res) => match serde_json::from_slice::<CommunityProfile>(&res) {
            Ok(profile) => {
                description = profile
                    .description
                    .as_ref()
                    .unwrap_or(&String::from(""))
                    .to_string();
                date = profile
                    .updated_at
                    .as_ref()
                    .unwrap_or(&Utc::now())
                    .date_naive();
            }
            Err(e) => log::error!("Error parsing Community Profile: {:?}", e),
        },
        None => log::error!(
            "Error fetching Community Profile: {:?}",
            community_profile_url
        ),
    }

    let mut payload = String::new();
    match get_readme(github_token, owner, repo).await {
        Some(content) => {
            let content = squeeze_fit_post_texts(&content, 12_000, 0.6);
            match analyze_readme(&content).await {
                Some(summary) => payload = summary,
                None => log::error!("Error parsing README.md: {}/{}", owner, repo),
            }
        }
        None => log::error!("Error fetching README.md: {}/{}", owner, repo),
    };
    if description.is_empty() && payload.is_empty() {
        return None;
    }

    if description.is_empty() {
        description = payload.clone();
    } else if payload.is_empty() {
        payload = description.clone();
    }

    Some(GitMemory {
        memory_type: MemoryType::Meta,
        name: format!("{}/{}", owner, repo),
        tag_line: description,
        source_url: community_profile_url,
        payload: payload,
        date: date,
    })
}

pub async fn process_issues(
    github_token: &str,
    inp_vec: Vec<Issue>,
    target_person: Option<String>,
) -> Option<(String, usize, Vec<GitMemory>)> {
    let mut issues_summaries = String::new();
    let mut git_memory_vec = vec![];

    for issue in &inp_vec {
        match analyze_issue_integrated(github_token, issue, target_person.clone()).await {
            None => {
                log::error!("Error analyzing issue: {:?}", issue.url.to_string());
                continue;
            }
            Some((summary, gm)) => {
                issues_summaries.push_str(&format!("{} {}\n", gm.date, summary));
                git_memory_vec.push(gm);
                if git_memory_vec.len() > 16 {
                    break;
                }
            }
        }
    }

    let count = git_memory_vec.len();
    if count == 0 {
        log::error!("No issues processed");
        return None;
    }
    Some((issues_summaries, count, git_memory_vec))
}
pub async fn analyze_readme(content: &str) -> Option<String> {
    let openai = OpenAIFlows::new();

    let sys_prompt_1 = &format!(
        "Your task is to objectively analyze a GitHub profile and the README of their project. Focus on extracting factual information about the features of the project, and its stated objectives. Avoid making judgments or inferring subjective value."
    );

    let co = ChatOptions {
        model: chat::ChatModel::GPT35Turbo16K,
        system_prompt: Some(sys_prompt_1),
        restart: true,
        temperature: Some(0.7),
        max_tokens: Some(256),
        ..Default::default()
    };
    let usr_prompt_1 = &format!(
        "Based on the profile and README provided: {content}, extract a concise summary detailing this project's factual significance in its domain, their areas of expertise, and the main features and goals of the project. Ensure the insights are objective and under 110 tokens."
    );

    match openai
        .chat_completion(&format!("profile-99"), usr_prompt_1, &co)
        .await
    {
        Ok(r) => Some(r.choice),
        Err(e) => {
            log::error!("Error summarizing meta data: {}", e);
            None
        }
    }
}

pub async fn analyze_issue_integrated(
    github_token: &str,
    issue: &Issue,
    target_person: Option<String>,
) -> Option<(String, GitMemory)> {
    let openai = OpenAIFlows::new();

    let issue_creator_name = &issue.user.login;
    let issue_title = issue.title.to_string();
    let issue_number = issue.number;
    let issue_date = issue.created_at.date_naive();

    let issue_body = match &issue.body {
        Some(body) => squeeze_fit_remove_quoted(body, "```", 500, 0.6),
        None => "".to_string(),
    };
    let issue_url = issue.url.to_string();
    let source_url = issue.html_url.to_string();

    let labels = issue
        .labels
        .iter()
        .map(|lab| lab.name.clone())
        .collect::<Vec<String>>()
        .join(", ");

    let mut all_text_from_issue = format!(
        "User '{}', opened an issue titled '{}', labeled '{}', with the following post: '{}'.",
        issue_creator_name, issue_title, labels, issue_body
    );

    let mut current_page = 1;
    loop {
        let url_str = format!("{}/comments?&page={}", issue_url, current_page);

        match github_http_fetch(&github_token, &url_str).await {
            Some(res) => match serde_json::from_slice::<Vec<Comment>>(res.as_slice()) {
                Err(_e) => {
                    log::error!(
                        "Error parsing Vec<Comment> at page {}: {:?}",
                        current_page,
                        _e
                    );
                    break;
                }
                Ok(comments_obj) => {
                    if comments_obj.is_empty() {
                        break;
                    }
                    for comment in &comments_obj {
                        let comment_body = match &comment.body {
                            Some(body) => squeeze_fit_remove_quoted(body, "```", 300, 0.6),
                            None => "".to_string(),
                        };
                        let commenter = &comment.user.login;
                        let commenter_input = format!("{} commented: {}", commenter, comment_body);

                        all_text_from_issue.push_str(&commenter_input);
                    }
                }
            },
            None => {
                break;
            }
        }

        current_page += 1;
    }
    let all_text_from_issue = squeeze_fit_remove_quoted(&all_text_from_issue, "```", 9000, 0.4);
    let target_str = target_person
        .clone()
        .unwrap_or("key participants".to_string());

    let sys_prompt_1 = &format!(
        "Given the information that user '{issue_creator_name}' opened an issue titled '{issue_title}', your task is to deeply analyze the content of the issue posts. Distill the crux of the issue, the potential solutions suggested, and evaluate the significant contributions of the participants in resolving or progressing the discussion."
    );

    let co = match all_text_from_issue.len() > 12000 {
        true => ChatOptions {
            model: chat::ChatModel::GPT35Turbo16K,
            system_prompt: Some(sys_prompt_1),
            restart: true,
            temperature: Some(0.7),
            max_tokens: Some(192),
            ..Default::default()
        },
        false => ChatOptions {
            model: chat::ChatModel::GPT35Turbo,
            system_prompt: Some(sys_prompt_1),
            restart: true,
            temperature: Some(0.7),
            max_tokens: Some(128),
            ..Default::default()
        },
    };
    let usr_prompt_1 = &format!(
        "Analyze the GitHub issue content: {all_text_from_issue}. Provide a concise analysis touching upon: The central problem discussed in the issue. The main solutions proposed or agreed upon. Emphasize the role and significance of '{target_str}' in contributing towards the resolution or progression of the discussion. Aim for a succinct, analytical summary that stays under 128 tokens."
    );

    match openai
        .chat_completion(&format!("issue_{issue_number}"), usr_prompt_1, &co)
        .await
    {
        Ok(r) => {
            let mut out = format!("{issue_url} ");
            out.push_str(&r.choice);
            let name = target_person
                .unwrap_or(issue_creator_name.to_string())
                .to_string();
            let gm = GitMemory {
                memory_type: MemoryType::Issue,
                name: name,
                tag_line: issue_title,
                source_url: source_url,
                payload: r.choice,
                date: issue_date,
            };

            Some((out, gm))
        }
        Err(_e) => {
            log::error!("Error generating issue summary #{}: {}", issue_number, _e);
            None
        }
    }
}

pub async fn analyze_commit_integrated(
    github_token: &str,
    user_name: &str,
    tag_line: &str,
    url: &str,
) -> Option<String> {
    let openai = OpenAIFlows::new();

    let commit_patch_str = format!("{url}.patch");
    let uri = http_req::uri::Uri::try_from(commit_patch_str.as_str())
        .expect(&format!("Error generating URI from {:?}", commit_patch_str));
    let mut writer = Vec::new();
    match http_req::request::Request::new(&uri)
        .method(http_req::request::Method::GET)
        .header("User-Agent", "flows-network connector")
        .header("Content-Type", "plain/text")
        .header("Authorization", &format!("Bearer {github_token}"))
        .send(&mut writer)
    {
        Ok(res) => {
            if !res.status_code().is_success() {
                log::error!("Github http error {:?}", res.status_code());
                return None;
            };

            let text = String::from_utf8_lossy(writer.as_slice());

            let mut stripped_texts = String::with_capacity(text.len());
            let mut inside_diff_block = false;

            let max_length = 24_000;

            for line in text.lines() {
                if stripped_texts.len() + line.len() > max_length {
                    let remaining = max_length - stripped_texts.len();
                    stripped_texts.push_str(&line.chars().take(remaining).collect::<String>());
                    break;
                }

                if line.starts_with("diff --git") {
                    inside_diff_block = true;
                    stripped_texts.push_str(line);
                    stripped_texts.push('\n');
                    continue;
                }

                if inside_diff_block {
                    if line
                        .chars()
                        .any(|ch| ch == '[' || ch == ']' || ch == '{' || ch == '}')
                    {
                        continue;
                    }
                }

                stripped_texts.push_str(line);
                stripped_texts.push('\n');

                if line.is_empty() {
                    inside_diff_block = false;
                }
            }
            let sys_prompt_1 = &format!(
                "Given a commit patch from the user {user_name}, you are to analyze its content. Focus on the core essence of the changes without delving into granular technical specifics. Particularly, identify the purpose of the changes, the files impacted, and the broader implications for the project. Remember to strike a balance between brevity and capturing the essential details."
            );

            let co = match stripped_texts.len() > 12000 {
                true => ChatOptions {
                    model: chat::ChatModel::GPT35Turbo16K,
                    system_prompt: Some(sys_prompt_1),
                    restart: true,
                    temperature: Some(0.7),
                    max_tokens: Some(192),
                    ..Default::default()
                },
                false => ChatOptions {
                    model: chat::ChatModel::GPT35Turbo,
                    system_prompt: Some(sys_prompt_1),
                    restart: true,
                    temperature: Some(0.7),
                    max_tokens: Some(128),
                    ..Default::default()
                },
            };
            let usr_prompt_1 = &format!(
                "Analyze the commit patch: {stripped_texts}, and its description: {tag_line}. Summarize the main changes, emphasizing the intent behind the modifications and their implications for the project. Ensure clarity, but avoid granular technical details. Distinguish between core code and other types of changes. Conclude with a brief evaluation of {user_name}'s contributions in this commit and its potential impact on the project. Keep your response concise and under 110 tokens."
            );

            let sha_serial = match url.rsplitn(2, "/").nth(0) {
                Some(s) => s.chars().take(5).collect::<String>(),
                None => "0000".to_string(),
            };
            match openai
                .chat_completion(&format!("commit-{sha_serial}"), usr_prompt_1, &co)
                .await
            {
                Ok(r) => {
                    let mut out = format!("{} ", url);
                    out.push_str(&r.choice);
                    Some(out)
                }
                Err(_e) => {
                    log::error!("Error generating issue summary #{}: {}", sha_serial, _e);
                    None
                }
            }
        }
        Err(_e) => {
            log::error!("Error getting response from Github: {:?}", _e);
            None
        }
    }
}

pub async fn process_commits(github_token: &str, inp_vec: &mut Vec<GitMemory>) -> Option<String> {
    let mut commits_summaries = String::new();

    let max_entries = 20; // Maximum entries to process
    let mut processed_count = 0; // Number of processed entries

    for commit_obj in inp_vec.iter_mut() {
        if processed_count >= max_entries {
            break;
        }

        match analyze_commit_integrated(
            github_token,
            &commit_obj.name,
            &commit_obj.tag_line,
            &commit_obj.source_url,
        )
        .await
        {
            Some(summary) => {
                commit_obj.payload = summary;

                if commits_summaries.len() <= 45_000 {
                    commits_summaries
                        .push_str(&format!("{} {}\n", commit_obj.date, commit_obj.payload));
                }

                processed_count += 1;
            }
            None => {
                log::error!(
                    "Error analyzing commit {:?} for user {}",
                    commit_obj.source_url,
                    commit_obj.name
                );
            }
        }
    }

    if processed_count == 0 {
        log::error!("No commits processed");
        return None;
    }

    Some(commits_summaries)
}

pub async fn correlate_commits_issues(
    _commits_summary: &str,
    _issues_summary: &str,
) -> Option<String> {
    let (commits_summary, issues_summary) =
        squeeze_fit_commits_issues(_commits_summary, _issues_summary, 0.6);

    let sys_prompt_1 = &format!(
        "Your task is to identify the 1-3 most impactful contributions by a specific user, based on the given commit logs and issue records. Pay close attention to any sequential relationships between issues and commits, and consider how they reflect the user's growth and evolution within the project. Use this data to evaluate the user's overall influence on the project's development. Provide a concise summary in bullet-point format."
    );

    let usr_prompt_1 = &format!(
        "Given the commit logs: {commits_summary} and issue records: {issues_summary}, identify the most significant contributions made by the user. Look for patterns and sequences of events that indicate the user's growth and how they approached problem-solving. Consider major code changes, and initiatives that had substantial impact on the project. Additionally, note any instances where the resolution of an issue led to a specific commit."
    );

    let usr_prompt_2 = &format!(
        "Based on the contributions identified, create a concise bullet-point summary. Highlight the user's key contributions and their influence on the project. Pay attention to their growth over time, and how their responses to issues evolved. Make sure to reference any interconnected events between issues and commits. Avoid replicating phrases from the source data and focus on providing a unique and insightful narrative. Please ensure your answer stayed below 256 tokens."
    );

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
    _profile_data: Option<&str>,
    _commits_summary: Option<&str>,
    _issues_summary: Option<&str>,
    _discussions_summary: Option<&str>,
    target_person: Option<&str>,
) -> Option<String> {
    let total_space = 16000; // 16k tokens

    let _total_ratio = 11.0; // 1 + 4 + 4 + 2
    let profile_ratio = 1.0;
    let commit_ratio = 4.0;
    let issue_ratio = 4.0;
    let discussion_ratio = 2.0;

    let available_ratios = [
        _profile_data.map(|_| profile_ratio),
        _commits_summary.map(|_| commit_ratio),
        _issues_summary.map(|_| issue_ratio),
        _discussions_summary.map(|_| discussion_ratio),
    ];

    let total_available_ratio: f32 = available_ratios.iter().filter_map(|&x| x).sum();

    let compute_space =
        |ratio: f32| -> usize { ((total_space as f32) * (ratio / total_available_ratio)) as usize };

    let profile_space = _profile_data.map_or(0, |_| compute_space(profile_ratio));
    let commit_space = _commits_summary.map_or(0, |_| compute_space(commit_ratio));
    let issue_space = _issues_summary.map_or(0, |_| compute_space(issue_ratio));
    let discussion_space = _discussions_summary.map_or(0, |_| compute_space(discussion_ratio));

    let trim_to_allocated_space =
        |source: &str, space: usize| -> String { source.chars().take(space * 3).collect() };

    let profile_str = _profile_data.map_or("".to_string(), |x| {
        format!(
            "profile data: {}",
            trim_to_allocated_space(x, profile_space)
        )
    });
    let commits_str = _commits_summary.map_or("".to_string(), |x| {
        format!("commit logs: {}", trim_to_allocated_space(x, commit_space))
    });
    let issues_str = _issues_summary.map_or("".to_string(), |x| {
        format!("issue post: {}", trim_to_allocated_space(x, issue_space))
    });
    let discussions_str = _discussions_summary.map_or("".to_string(), |x| {
        format!(
            "discussion posts: {}",
            trim_to_allocated_space(x, discussion_space)
        )
    });

    let target_str = match target_person {
        Some(person) => format!("{}'s", person),
        None => "key participants'".to_string(),
    };

    let sys_prompt_1 =
        "Analyze the GitHub activity data and profile data over the week to detect both key impactful contributions and connections between commits, issues, and discussions. Highlight specific code changes, resolutions, and improvements. Furthermore, trace evidence of commits addressing specific issues, discussions leading to commits, or issues spurred by discussions. The aim is to map out both the impactful technical advancements and the developmental narrative of the project.";

    let usr_prompt_1 = &format!(
        "From {profile_str}, {commits_str}, {issues_str}, and {discussions_str}, detail {target_str}'s significant technical contributions. Enumerate individual tasks, code enhancements, and bug resolutions, emphasizing impactful contributions. Concurrently, identify connections: commits that appear to resolve specific issues, discussions that may have catalyzed certain commits, or issues influenced by preceding discussions. Extract tangible instances showcasing both impact and interconnections within the week."
    );

    let usr_prompt_2 = &format!(
        "Merge the identified impactful technical contributions and their interconnections into a coherent summary for {target_str} over the week. Describe how these contributions align with the project's technical objectives. Pinpoint recurring technical patterns or trends and shed light on the synergy between individual efforts and their collective progression. Detail both the weight of each contribution and their interconnectedness in shaping the project. Limit to 256 tokens."
    );

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

    let sys_prompt_1 = &format!(
        "First, let's analyze and understand the provided Github data in a step-by-step manner. Begin by evaluating the user's activity based on their most active repositories, languages used, issues they're involved in, and discussions they've participated in. Concurrently, grasp the characteristics and requirements of the home project. Your aim is to identify overlaps or connections between the user's skills or activities and the home project's needs."
    );

    let usr_prompt_1 = &format!(
        "Using a structured approach, analyze the given data: User Profile: {} Active Repositories: {} Issues Involved: {} Discussions Participated: {} Home project's characteristics: {} Identify patterns in the user's activity and spot potential synergies with the home project. Pay special attention to the programming languages they use, especially if they align with the home project's requirements. Derive insights from their interactions and the data provided.",
        user_profile,
        repos_data,
        issues_data,
        discussion_data,
        home_repo_data
    );

    let usr_prompt_2 = &format!(
        "Now, using the insights from your step-by-step analysis, craft a concise bullet-point summary that underscores: - The user's main areas of expertise and interest. - The relevance of their preferred languages or technologies to the home project. - Their potential contributions to the home project, based on their skills and interactions. Ensure the summary is clear, insightful, and remains under 256 tokens. Emphasize any evident alignments between the user's skills and the project's needs."
    );
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
