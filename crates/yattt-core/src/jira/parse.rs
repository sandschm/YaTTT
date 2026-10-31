//! Conversion of Jira JSON payloads into the local model. Handles the differences between
//! Jira Cloud and Data Center where they matter (sprint field representation, user ids).

use crate::models::*;
use serde_json::Value;

/// Custom field ids discovered via `/rest/api/2/field`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldIds {
    pub sprint: Option<String>,
    pub epic_link: Option<String>,
}

impl FieldIds {
    pub fn from_fields(fields: &Value) -> FieldIds {
        let mut ids = FieldIds::default();
        if let Some(arr) = fields.as_array() {
            for f in arr {
                let custom = f
                    .pointer("/schema/custom")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let id = f.get("id").and_then(Value::as_str).map(str::to_string);
                match custom {
                    "com.pyxis.greenhopper.jira:gh-sprint" => ids.sprint = id,
                    "com.pyxis.greenhopper.jira:gh-epic-link" => ids.epic_link = id,
                    _ => {}
                }
            }
        }
        ids
    }

    /// Field list for issue search requests.
    pub fn search_fields(&self) -> String {
        let mut fields = vec![
            "summary",
            "issuetype",
            "status",
            "parent",
            "assignee",
            "priority",
            "duedate",
            "timetracking",
            "updated",
            "project",
            "labels",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        if let Some(s) = &self.sprint {
            fields.push(s.clone());
        }
        if let Some(e) = &self.epic_link {
            fields.push(e.clone());
        }
        fields.join(",")
    }
}

fn str_at<'a>(v: &'a Value, pointer: &str) -> Option<&'a str> {
    v.pointer(pointer).and_then(Value::as_str)
}

pub fn parse_server_info(v: &Value) -> ServerInfo {
    let deployment = match str_at(v, "/deploymentType") {
        Some("Cloud") => Deployment::Cloud,
        Some(_) => Deployment::DataCenter,
        None => Deployment::Unknown,
    };
    ServerInfo {
        base_url: str_at(v, "/baseUrl").unwrap_or("").to_string(),
        version: str_at(v, "/version").unwrap_or("").to_string(),
        deployment,
        server_title: str_at(v, "/serverTitle").map(str::to_string),
    }
}

pub fn parse_user(v: &Value) -> Option<JiraUser> {
    let id = str_at(v, "/accountId")
        .or_else(|| str_at(v, "/name"))
        .or_else(|| str_at(v, "/key"))?
        .to_string();
    Some(JiraUser {
        id,
        key: str_at(v, "/key").map(str::to_string),
        display_name: str_at(v, "/displayName").unwrap_or("").to_string(),
        email: str_at(v, "/emailAddress").map(str::to_string),
        avatar_url: str_at(v, "/avatarUrls/48x48").map(str::to_string),
        time_zone: str_at(v, "/timeZone").map(str::to_string),
    })
}

pub fn parse_project(v: &Value) -> Option<Project> {
    Some(Project {
        id: str_at(v, "/id")?.to_string(),
        key: str_at(v, "/key")?.to_string(),
        name: str_at(v, "/name").unwrap_or("").to_string(),
        avatar_url: str_at(v, "/avatarUrls/48x48").map(str::to_string),
        color: None,
        selected: false,
    })
}

pub fn parse_board(v: &Value) -> Option<Board> {
    Some(Board {
        id: v.get("id")?.as_i64()?,
        name: str_at(v, "/name").unwrap_or("").to_string(),
        board_type: str_at(v, "/type").unwrap_or("").to_string(),
        project_key: str_at(v, "/location/projectKey").map(str::to_string),
    })
}

pub fn parse_sprint(v: &Value) -> Option<Sprint> {
    Some(Sprint {
        id: v.get("id")?.as_i64()?,
        name: str_at(v, "/name").unwrap_or("").to_string(),
        state: str_at(v, "/state").unwrap_or("").to_lowercase(),
        board_id: v.get("originBoardId").and_then(Value::as_i64),
        start_date: str_at(v, "/startDate").map(str::to_string),
        end_date: str_at(v, "/endDate").map(str::to_string),
        goal: str_at(v, "/goal").map(str::to_string),
        selected: false,
    })
}

pub fn parse_issue_type(v: &Value) -> Option<IssueType> {
    Some(IssueType {
        id: str_at(v, "/id")?.to_string(),
        name: str_at(v, "/name").unwrap_or("").to_string(),
        subtask: v.get("subtask").and_then(Value::as_bool).unwrap_or(false),
        icon_url: str_at(v, "/iconUrl").map(str::to_string),
    })
}

/// Sprint information as (id, name, state). Picks the active sprint if there are several.
pub fn parse_sprint_field(v: &Value) -> Option<(i64, String, String)> {
    let arr = v.as_array()?;
    let mut parsed: Vec<(i64, String, String)> =
        arr.iter().filter_map(parse_single_sprint_value).collect();
    if parsed.is_empty() {
        return None;
    }
    if let Some(pos) = parsed.iter().position(|(_, _, state)| state == "active") {
        return Some(parsed.remove(pos));
    }
    if let Some(pos) = parsed.iter().position(|(_, _, state)| state == "future") {
        return Some(parsed.remove(pos));
    }
    parsed.pop()
}

fn parse_single_sprint_value(v: &Value) -> Option<(i64, String, String)> {
    if let Some(obj) = v.as_object() {
        let id = obj.get("id")?.as_i64()?;
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let state = obj
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        return Some((id, name, state));
    }
    // Data Center: "com.atlassian.greenhopper.service.sprint.Sprint@1a2b[id=12,rapidViewId=3,state=ACTIVE,name=Sprint 7,startDate=...]"
    let s = v.as_str()?;
    let body = s.find('[').map(|i| &s[i + 1..]).unwrap_or(s);
    let body = body.strip_suffix(']').unwrap_or(body);
    let id = extract_tostring_field(body, "id")?.parse::<i64>().ok()?;
    let state = extract_tostring_field(body, "state")
        .unwrap_or_default()
        .to_lowercase();
    let name = extract_tostring_field(body, "name").unwrap_or_default();
    Some((id, name, state))
}

/// Extracts `key=value` from Jira's toString representation. Values run until the next
/// `,<knownKey>=` to tolerate commas in sprint names.
fn extract_tostring_field(body: &str, key: &str) -> Option<String> {
    const KEYS: &[&str] = &[
        "id",
        "rapidViewId",
        "state",
        "name",
        "startDate",
        "endDate",
        "completeDate",
        "activatedDate",
        "sequence",
        "goal",
        "autoStartStop",
        "synced",
        "incompleteIssuesDestinationId",
    ];
    let needle = format!("{key}=");
    let start = if body.starts_with(&needle) {
        0
    } else {
        body.find(&format!(",{needle}")).map(|i| i + 1)?
    };
    let value_start = start + needle.len();
    let rest = &body[value_start..];
    let mut end = rest.len();
    for k in KEYS {
        if *k == key {
            continue;
        }
        if let Some(i) = rest.find(&format!(",{k}=")) {
            end = end.min(i);
        }
    }
    Some(rest[..end].to_string())
}

pub fn parse_issue(v: &Value, ids: &FieldIds) -> Option<Issue> {
    let key = str_at(v, "/key")?.to_string();
    let id = v.get("id").map(|i| {
        i.as_str()
            .map(str::to_string)
            .unwrap_or_else(|| i.to_string())
    })?;
    let fields = v.get("fields")?;
    let project_key = str_at(fields, "/project/key")
        .map(str::to_string)
        .unwrap_or_else(|| {
            key.rsplit_once('-')
                .map(|(p, _)| p.to_string())
                .unwrap_or_default()
        });
    let (sprint_id, sprint_name) = ids
        .sprint
        .as_ref()
        .and_then(|f| fields.get(f))
        .and_then(parse_sprint_field)
        .map(|(id, name, _)| (Some(id), Some(name)))
        .unwrap_or((None, None));
    let parent_key = str_at(fields, "/parent/key").map(str::to_string);
    let parent_is_epic = str_at(fields, "/parent/fields/issuetype/name")
        .map(|n| n.eq_ignore_ascii_case("epic"))
        .unwrap_or(false);
    let epic_key = ids
        .epic_link
        .as_ref()
        .and_then(|f| fields.get(f))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            if parent_is_epic {
                parent_key.clone()
            } else {
                None
            }
        });
    let is_subtask = fields
        .pointer("/issuetype/subtask")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(Issue {
        id,
        key,
        project_key,
        summary: str_at(fields, "/summary").unwrap_or("").to_string(),
        issue_type: str_at(fields, "/issuetype/name").unwrap_or("").to_string(),
        issue_type_id: str_at(fields, "/issuetype/id").map(str::to_string),
        issue_type_icon: str_at(fields, "/issuetype/iconUrl").map(str::to_string),
        is_subtask,
        status: str_at(fields, "/status/name").unwrap_or("").to_string(),
        status_category: str_at(fields, "/status/statusCategory/key")
            .unwrap_or("undefined")
            .to_string(),
        parent_key: if is_subtask || !parent_is_epic {
            parent_key
        } else {
            None
        },
        epic_key,
        sprint_id,
        sprint_name,
        assignee: str_at(fields, "/assignee/displayName").map(str::to_string),
        assignee_id: str_at(fields, "/assignee/accountId")
            .or_else(|| str_at(fields, "/assignee/name"))
            .or_else(|| str_at(fields, "/assignee/key"))
            .map(str::to_string),
        priority: str_at(fields, "/priority/name").map(str::to_string),
        due_date: str_at(fields, "/duedate").map(str::to_string),
        original_estimate_s: fields
            .pointer("/timetracking/originalEstimateSeconds")
            .and_then(Value::as_i64),
        remaining_estimate_s: fields
            .pointer("/timetracking/remainingEstimateSeconds")
            .and_then(Value::as_i64),
        time_spent_s: fields
            .pointer("/timetracking/timeSpentSeconds")
            .and_then(Value::as_i64),
        updated: str_at(fields, "/updated").map(str::to_string),
        labels: fields
            .get("labels")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        favorite: false,
        source: "sprint".into(),
        last_seen: None,
    })
}

/// A worklog as returned by Jira.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteWorklog {
    pub id: String,
    pub issue_key: String,
    pub author_id: Option<String>,
    pub started: String,
    pub time_spent_seconds: i64,
    pub comment: String,
}

pub fn parse_worklog(v: &Value, issue_key: &str) -> Option<RemoteWorklog> {
    let id = v.get("id").map(|i| {
        i.as_str()
            .map(str::to_string)
            .unwrap_or_else(|| i.to_string())
    })?;
    let comment = match v.get("comment") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(_)) => adf_to_text(v.get("comment").unwrap()),
        _ => String::new(),
    };
    Some(RemoteWorklog {
        id,
        issue_key: issue_key.to_string(),
        author_id: str_at(v, "/author/accountId")
            .or_else(|| str_at(v, "/author/name"))
            .or_else(|| str_at(v, "/author/key"))
            .map(str::to_string),
        started: str_at(v, "/started")?.to_string(),
        time_spent_seconds: v
            .get("timeSpentSeconds")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        comment,
    })
}

/// Flatten an Atlassian Document Format node to plain text.
pub fn adf_to_text(v: &Value) -> String {
    let mut out = String::new();
    fn walk(v: &Value, out: &mut String) {
        if let Some(t) = v.get("text").and_then(Value::as_str) {
            out.push_str(t);
        }
        if let Some(children) = v.get("content").and_then(Value::as_array) {
            for (i, c) in children.iter().enumerate() {
                if i > 0 && c.get("type").and_then(Value::as_str) == Some("paragraph") {
                    out.push('\n');
                }
                walk(c, out);
            }
        }
    }
    walk(v, &mut out);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_dc_sprint_string() {
        let v = json!(["com.atlassian.greenhopper.service.sprint.Sprint@6b1[id=42,rapidViewId=7,state=ACTIVE,name=Sprint 12, Teil 2,startDate=2026-10-20T08:00:00.000Z,endDate=2026-11-03T08:00:00.000Z,completeDate=<null>,activatedDate=2026-10-20T08:00:00.000Z,sequence=42,goal=Ship it,autoStartStop=false,synced=false]"]);
        let (id, name, state) = parse_sprint_field(&v).unwrap();
        assert_eq!(id, 42);
        assert_eq!(name, "Sprint 12, Teil 2");
        assert_eq!(state, "active");
    }

    #[test]
    fn prefers_active_sprint_from_cloud_objects() {
        let v = json!([
            {"id": 1, "name": "Old", "state": "closed"},
            {"id": 2, "name": "Now", "state": "active"},
            {"id": 3, "name": "Next", "state": "future"}
        ]);
        assert_eq!(
            parse_sprint_field(&v).unwrap(),
            (2, "Now".into(), "active".into())
        );
    }

    #[test]
    fn parses_issue_with_estimates_and_parent() {
        let ids = FieldIds {
            sprint: Some("customfield_10020".into()),
            epic_link: None,
        };
        let v = json!({
            "id": "10001",
            "key": "ABC-7",
            "fields": {
                "summary": "Do things",
                "project": {"key": "ABC"},
                "issuetype": {"id": "3", "name": "Sub-task", "subtask": true, "iconUrl": "https://x/i.png"},
                "status": {"name": "In Progress", "statusCategory": {"key": "indeterminate"}},
                "parent": {"key": "ABC-1", "fields": {"issuetype": {"name": "Story"}}},
                "assignee": {"accountId": "acc-1", "displayName": "Sam"},
                "timetracking": {"originalEstimateSeconds": 7200, "timeSpentSeconds": 600},
                "customfield_10020": [{"id": 9, "name": "S9", "state": "active"}],
                "updated": "2026-10-30T10:00:00.000+0100"
            }
        });
        let issue = parse_issue(&v, &ids).unwrap();
        assert_eq!(issue.project_key, "ABC");
        assert!(issue.is_subtask);
        assert_eq!(issue.parent_key.as_deref(), Some("ABC-1"));
        assert_eq!(issue.sprint_id, Some(9));
        assert_eq!(issue.original_estimate_s, Some(7200));
        assert_eq!(issue.assignee_id.as_deref(), Some("acc-1"));
        assert_eq!(issue.status_category, "indeterminate");
    }

    #[test]
    fn epic_parent_becomes_epic_key() {
        let ids = FieldIds::default();
        let v = json!({
            "id": 5, "key": "ABC-9",
            "fields": {
                "summary": "Story", "issuetype": {"name": "Story", "subtask": false},
                "status": {"name": "To Do", "statusCategory": {"key": "new"}},
                "parent": {"key": "ABC-2", "fields": {"issuetype": {"name": "Epic"}}}
            }
        });
        let issue = parse_issue(&v, &ids).unwrap();
        assert_eq!(issue.epic_key.as_deref(), Some("ABC-2"));
        assert_eq!(issue.parent_key, None);
        assert_eq!(issue.id, "5");
    }

    #[test]
    fn field_discovery() {
        let v = json!([
            {"id": "customfield_10020", "name": "Sprint", "schema": {"custom": "com.pyxis.greenhopper.jira:gh-sprint"}},
            {"id": "customfield_10014", "name": "Epic Link", "schema": {"custom": "com.pyxis.greenhopper.jira:gh-epic-link"}},
            {"id": "summary", "name": "Summary"}
        ]);
        let ids = FieldIds::from_fields(&v);
        assert_eq!(ids.sprint.as_deref(), Some("customfield_10020"));
        assert_eq!(ids.epic_link.as_deref(), Some("customfield_10014"));
        assert!(ids.search_fields().contains("customfield_10014"));
    }

    #[test]
    fn adf_flattening() {
        let v = json!({"type": "doc", "content": [
            {"type": "paragraph", "content": [{"type": "text", "text": "Hello "}, {"type": "text", "text": "world"}]},
            {"type": "paragraph", "content": [{"type": "text", "text": "Second"}]}
        ]});
        assert_eq!(adf_to_text(&v), "Hello world\nSecond");
    }

    #[test]
    fn user_ids_cloud_and_dc() {
        let cloud = json!({"accountId": "abc", "displayName": "A", "emailAddress": "a@x.de"});
        let dc = json!({"name": "auser", "key": "JIRAUSER1", "displayName": "A"});
        assert_eq!(parse_user(&cloud).unwrap().id, "abc");
        let dc_user = parse_user(&dc).unwrap();
        assert_eq!(dc_user.id, "auser");
        assert_eq!(dc_user.key.as_deref(), Some("JIRAUSER1"));
    }
}
