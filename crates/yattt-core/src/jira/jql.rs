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
    // Per-project label filter: "(project not in (X, Y) OR (project = X AND labels in (...)) OR ...)".
    let labelled: Vec<(String, Vec<String>)> = sel
        .project_labels
        .keys()
        .map(|k| (k.clone(), sel.labels_for(k)))
        .filter(|(_, labels)| !labels.is_empty())
        .collect();
    if !labelled.is_empty() {
        let keys = labelled
            .iter()
            .map(|(k, _)| quote(k))
            .collect::<Vec<_>>()
            .join(", ");
        let mut alternatives = vec![format!("project not in ({keys})")];
        for (key, labels) in &labelled {
            let values = labels
                .iter()
                .map(|l| quote(l))
                .collect::<Vec<_>>()
                .join(", ");
            alternatives.push(format!(
                "(project = {} AND labels in ({values}))",
                quote(key)
            ));
        }
        clauses.push(format!("({})", alternatives.join(" OR ")));
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
            project_labels: Default::default(),
        };
        let jql = build_issue_jql(&sel).unwrap();
        assert_eq!(
            jql,
            "((project in (\"ABC\", \"XY\") AND sprint in openSprints()) OR sprint in (12, 13)) AND assignee = currentUser() AND statusCategory != Done AND issuetype not in (\"Epic\", \"Sub \\\"task\\\"\") AND (labels = urgent) ORDER BY updated DESC"
        );
    }

    #[test]
    fn labels_restrict_only_the_configured_project() {
        let mut project_labels = std::collections::BTreeMap::new();
        project_labels.insert(
            "SHARED".to_string(),
            vec![
                "team-blau".to_string(),
                " hr-core ".to_string(),
                "".to_string(),
            ],
        );
        // not selected any more: must be ignored
        project_labels.insert("OLD".to_string(), vec!["x".to_string()]);
        let mut sel = SelectionSettings {
            project_keys: vec!["SHARED".into(), "ABC".into()],
            project_labels,
            excluded_issue_types: vec![],
            hide_done: false,
            ..Default::default()
        };
        sel.tidy_project_labels();
        let jql = build_issue_jql(&sel).unwrap();
        assert_eq!(
            jql,
            "((project in (\"SHARED\", \"ABC\") AND sprint in openSprints())) AND (project not in (\"SHARED\") OR (project = \"SHARED\" AND labels in (\"team-blau\", \"hr-core\"))) ORDER BY updated DESC"
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
