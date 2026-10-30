//! Builds the JQL query for the ticket list from the selection settings.

use crate::settings::SelectionSettings;

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Returns `None` when nothing is selected (no projects and no sprints).
pub fn build_issue_jql(sel: &SelectionSettings) -> Option<String> {
    let mut scopes: Vec<String> = Vec::new();
    if !sel.project_keys.is_empty() && sel.include_open_sprints {
        let keys = sel
            .project_keys
            .iter()
            .map(|k| quote(k))
            .collect::<Vec<_>>()
            .join(", ");
        scopes.push(format!("(project in ({keys}) AND sprint in openSprints())"));
    }
    if !sel.sprint_ids.is_empty() {
        let ids = sel
            .sprint_ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        scopes.push(format!("sprint in ({ids})"));
    }
    if scopes.is_empty() {
        return None;
    }
    let mut clauses = vec![format!("({})", scopes.join(" OR "))];
    if sel.only_assigned_to_me {
        clauses.push("assignee = currentUser()".into());
    }
    if sel.hide_done {
        clauses.push("statusCategory != Done".into());
    }
    if !sel.excluded_issue_types.is_empty() {
        let types = sel
            .excluded_issue_types
            .iter()
            .map(|t| quote(t))
            .collect::<Vec<_>>()
            .join(", ");
        clauses.push(format!("issuetype not in ({types})"));
    }
    let extra = sel.extra_jql.trim();
    if !extra.is_empty() {
        clauses.push(format!("({extra})"));
    }
    Some(format!("{} ORDER BY updated DESC", clauses.join(" AND ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_selected() {
        let sel = SelectionSettings {
            project_keys: vec![],
            sprint_ids: vec![],
            ..Default::default()
        };
        assert!(build_issue_jql(&sel).is_none());
    }

    #[test]
    fn projects_and_sprints() {
        let sel = SelectionSettings {
            project_keys: vec!["ABC".into(), "XY".into()],
            sprint_ids: vec![12, 13],
            excluded_issue_types: vec!["Epic".into(), "Sub \"task\"".into()],
            include_open_sprints: true,
            only_assigned_to_me: true,
            hide_done: true,
            extra_jql: "labels = urgent".into(),
        };
        let jql = build_issue_jql(&sel).unwrap();
        assert_eq!(
            jql,
            "((project in (\"ABC\", \"XY\") AND sprint in openSprints()) OR sprint in (12, 13)) AND assignee = currentUser() AND statusCategory != Done AND issuetype not in (\"Epic\", \"Sub \\\"task\\\"\") AND (labels = urgent) ORDER BY updated DESC"
        );
    }

    #[test]
    fn projects_without_open_sprints_need_sprint_selection() {
        let sel = SelectionSettings {
            project_keys: vec!["ABC".into()],
            include_open_sprints: false,
            ..Default::default()
        };
        assert!(build_issue_jql(&sel).is_none());
    }
}
