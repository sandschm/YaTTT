//! Builds the JQL query for the ticket list from the selection settings.

use crate::settings::{ProjectFilter, SelectionSettings};

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
    // Per-project filter: "(project not in (X, Y) OR (project = X AND labels in (...)) OR ...)".
    let filtered: Vec<(String, ProjectFilter)> = sel
        .project_filters
        .keys()
        .filter_map(|k| sel.filter_for(k).map(|f| (k.clone(), f)))
        .collect();
    if !filtered.is_empty() {
        let keys = filtered
            .iter()
            .map(|(k, _)| quote(k))
            .collect::<Vec<_>>()
            .join(", ");
        let mut alternatives = vec![format!("project not in ({keys})")];
        for (key, filter) in &filtered {
            let mut conditions = Vec::new();
            if !filter.labels.is_empty() {
                let values = filter
                    .labels
                    .iter()
                    .map(|l| quote(l))
                    .collect::<Vec<_>>()
                    .join(", ");
                conditions.push(format!("labels in ({values})"));
            }
            if !filter.components.is_empty() {
                let values = filter
                    .components
                    .iter()
                    .map(|c| quote(c))
                    .collect::<Vec<_>>()
                    .join(", ");
                conditions.push(format!("component in ({values})"));
            }
            let joined = if conditions.len() == 1 {
                conditions.remove(0)
            } else {
                let op = if filter.match_all { " AND " } else { " OR " };
                format!("({})", conditions.join(op))
            };
            alternatives.push(format!("(project = {} AND {joined})", quote(key)));
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
            project_filters: Default::default(),
            project_labels: Default::default(),
        };
        let jql = build_issue_jql(&sel).unwrap();
        assert_eq!(
            jql,
            "((project in (\"ABC\", \"XY\") AND sprint in openSprints()) OR sprint in (12, 13)) AND assignee = currentUser() AND statusCategory != Done AND issuetype not in (\"Epic\", \"Sub \\\"task\\\"\") AND (labels = urgent) ORDER BY updated DESC"
        );
    }

    #[test]
    fn filters_restrict_only_the_configured_projects() {
        let mut project_filters = std::collections::BTreeMap::new();
        project_filters.insert(
            "SHARED".to_string(),
            ProjectFilter {
                labels: vec!["team-blau".into(), " hr-core ".into(), "".into()],
                components: vec!["Time Tracking".into()],
                match_all: true,
            },
        );
        project_filters.insert(
            "PAY".to_string(),
            ProjectFilter {
                labels: vec![],
                components: vec!["Payroll".into()],
                match_all: true,
            },
        );
        // not selected any more: must be ignored
        project_filters.insert(
            "OLD".to_string(),
            ProjectFilter {
                labels: vec!["x".into()],
                ..Default::default()
            },
        );
        let mut sel = SelectionSettings {
            project_keys: vec!["SHARED".into(), "PAY".into(), "ABC".into()],
            project_filters,
            excluded_issue_types: vec![],
            hide_done: false,
            ..Default::default()
        };
        sel.tidy_project_filters();
        let jql = build_issue_jql(&sel).unwrap();
        assert_eq!(
            jql,
            "((project in (\"SHARED\", \"PAY\", \"ABC\") AND sprint in openSprints())) AND (project not in (\"SHARED\", \"PAY\") OR (project = \"SHARED\" AND (labels in (\"team-blau\", \"hr-core\") AND component in (\"Time Tracking\"))) OR (project = \"PAY\" AND component in (\"Payroll\"))) ORDER BY updated DESC"
        );
        // "one of them is enough" joins with OR
        sel.project_filters.get_mut("SHARED").unwrap().match_all = false;
        let jql = build_issue_jql(&sel).unwrap();
        assert!(jql.contains(
            "(labels in (\"team-blau\", \"hr-core\") OR component in (\"Time Tracking\"))"
        ));
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
