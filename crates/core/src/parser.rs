use crate::model::{Analysis, Diagnostic, Edit, Severity};
use tree_sitter::Parser;

/// Parses Apex using the pinned sfapex grammar.  An erroneous tree is never
/// eligible for rewriting: callers receive a byte-accurate diagnostic instead.
pub fn analyze(source: &str) -> Analysis {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_sfapex::apex::LANGUAGE.into())
        .expect("pinned grammar is compatible with tree-sitter");
    let tree = match parser.parse(source, None) {
        Some(tree) => tree,
        None => {
            return Analysis {
                edits: vec![],
                diagnostics: vec![diagnostic(
                    "PARSER_CANCELLED",
                    0,
                    0,
                    "The Apex parser was cancelled.",
                )],
            };
        }
    };
    let root = tree.root_node();
    if root.has_error() {
        let error = first_error(root).unwrap_or(root);
        return Analysis {
            edits: vec![],
            diagnostics: vec![diagnostic(
                "PARSE_ERROR",
                error.start_byte(),
                error.end_byte(),
                "Apex could not be parsed; the file was left unchanged.",
            )],
        };
    }
    let mut analysis = Analysis::default();
    collect_native_dml(root, &mut analysis.edits);
    collect_database_access_levels(root, source, &mut analysis);
    collect_static_soql(root, source, &mut analysis);
    analysis
}

/// Only the body immediately inside brackets owns the execution mode. Child
/// relationship queries and semi-joins inherit it and must not get a WITH.
fn collect_static_soql(node: tree_sitter::Node<'_>, source: &str, analysis: &mut Analysis) {
    if node.kind() == "soql_query_body"
        && node
            .parent()
            .is_some_and(|parent| parent.kind() == "query_expression")
    {
        if let Some(with) = node.child_by_field_name("with_clause") {
            let mut cursor = with.walk();
            let explicit_mode = with.named_children(&mut cursor).any(|child| {
                child.kind() == "with_type"
                    && node_text(Some(child), source).is_some_and(|text| {
                        text.eq_ignore_ascii_case("USER_MODE")
                            || text.eq_ignore_ascii_case("SYSTEM_MODE")
                    })
            });
            if !explicit_mode {
                analysis.diagnostics.push(Diagnostic {
                    code: "UNSUPPORTED_SOQL_WITH",
                    severity: Severity::Warning,
                    start_byte: with.start_byte(),
                    end_byte: with.end_byte(),
                    message:
                        "SOQL already has a different WITH clause; the query was left unchanged."
                            .to_owned(),
                    suggestion: Some(
                        "Review the existing WITH clause before choosing an explicit access mode."
                            .to_owned(),
                    ),
                });
            }
        } else if !query_has_call_mode(node, source)
            && let Some(anchor) = node
                .child_by_field_name("where_clause")
                .or_else(|| node.child_by_field_name("using_clause"))
                .or_else(|| node.child_by_field_name("from_clause"))
        {
            // WITH follows WHERE/USING/FROM and precedes GROUP/ORDER/LIMIT,
            // OFFSET, FOR, UPDATE and ALL ROWS. Keep all existing source bytes.
            analysis.edits.push(Edit {
                start_byte: anchor.end_byte(),
                end_byte: anchor.end_byte(),
                replacement: " WITH SYSTEM_MODE".to_owned(),
                rule_id: "static-soql",
            });
        }
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_static_soql(child, source, analysis);
        }
    }
}

fn collect_native_dml(node: tree_sitter::Node<'_>, edits: &mut Vec<Edit>) {
    if node.kind() == "dml_expression"
        && node.child_by_field_name("security_mode").is_none()
        && let Some(kind) = (0..node.child_count())
            .filter_map(|index| node.child(index))
            .find(|child| child.kind() == "dml_type")
    {
        // The grammar's dml_type span is the verb only. Inserting immediately
        // after it preserves whitespace/comments and avoids text-wide matching.
        edits.push(Edit {
            start_byte: kind.end_byte(),
            end_byte: kind.end_byte(),
            replacement: " as system".to_owned(),
            rule_id: "native-dml",
        });
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_native_dml(child, edits);
        }
    }
}

/// Adds an explicit AccessLevel to supported `Database` overloads.
///
/// The call shape is identified by grammar fields, never by a source-wide text
/// search. Query calls coordinate with inline SOQL and only gain an argument
/// when resolved text has no mode. Unknown policies are never overwritten.
fn collect_database_access_levels(
    node: tree_sitter::Node<'_>,
    source: &str,
    analysis: &mut Analysis,
) {
    if node.kind() == "method_invocation"
        && node_text(node.child_by_field_name("object"), source)
            .is_some_and(|object| object.eq_ignore_ascii_case("Database"))
        && let Some(name) = node_text(node.child_by_field_name("name"), source)
        && let Some(arguments) = node.child_by_field_name("arguments")
    {
        if is_query_method(name) {
            collect_query_call(name, arguments, source, analysis);
        } else if let Some(rule_id) = database_access_level_rule(name, arguments, source) {
            let values = operands(arguments);
            let mode_position = if name.eq_ignore_ascii_case("merge") {
                2
            } else {
                1
            };
            if values.len() > mode_position
                && !node_text(values.last().copied().map(unwrapped), source).is_some_and(|text| {
                    text.eq_ignore_ascii_case("true") || text.eq_ignore_ascii_case("false")
                })
            {
                analysis.diagnostics.push(Diagnostic {
                    code: "AMBIGUOUS_DML_ACCESS_LEVEL", severity: Severity::Warning,
                    start_byte: arguments.start_byte(), end_byte: arguments.end_byte(),
                    message: "An existing DML argument may already provide an access level; the call was left unchanged.".to_owned(),
                    suggestion: Some("Review the overload and argument types before adding an access level.".to_owned()),
                });
            } else {
                append_access_level(arguments, rule_id, &mut analysis.edits);
            }
        }
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_database_access_levels(child, source, analysis);
        }
    }
}

fn append_access_level(
    arguments: tree_sitter::Node<'_>,
    rule_id: &'static str,
    edits: &mut Vec<Edit>,
) {
    edits.push(Edit {
        start_byte: arguments.end_byte() - 1,
        end_byte: arguments.end_byte() - 1,
        replacement: ", System.AccessLevel.SYSTEM_MODE".to_owned(),
        rule_id,
    });
}

// Comments are named nodes too, but never count as arguments or modes.
fn operands(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !child.is_extra())
        .collect()
}

fn unwrapped(mut node: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    while node.kind() == "parenthesized_expression" {
        let children = operands(node);
        if children.len() != 1 {
            break;
        }
        node = children[0];
    }
    node
}

fn is_query_method(name: &str) -> bool {
    [
        "query",
        "countQuery",
        "getQueryLocator",
        "queryWithBinds",
        "countQueryWithBinds",
        "getQueryLocatorWithBinds",
    ]
    .iter()
    .any(|method| name.eq_ignore_ascii_case(method))
}

fn query_has_call_mode(body: tree_sitter::Node<'_>, source: &str) -> bool {
    let Some(mut query) = body.parent() else {
        return false;
    };
    while let Some(parent) = query.parent() {
        if parent.kind() != "parenthesized_expression" {
            break;
        }
        query = parent;
    }
    let Some(args) = query
        .parent()
        .filter(|parent| parent.kind() == "argument_list")
    else {
        return false;
    };
    let Some(call) = args
        .parent()
        .filter(|parent| parent.kind() == "method_invocation")
    else {
        return false;
    };
    let values = operands(args);
    values.len() == 2
        && values[0] == query
        && node_text(call.child_by_field_name("object"), source)
            .is_some_and(|text| text.eq_ignore_ascii_case("Database"))
        && node_text(call.child_by_field_name("name"), source)
            .is_some_and(|text| text.eq_ignore_ascii_case("getQueryLocator"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryMode {
    Missing,
    User,
    System,
    Other,
}

fn query_mode(body: tree_sitter::Node<'_>, source: &str) -> QueryMode {
    let Some(with) = body.child_by_field_name("with_clause") else {
        return QueryMode::Missing;
    };
    for child in operands(with) {
        if child.kind() == "with_type" {
            match node_text(Some(child), source)
                .unwrap_or("")
                .to_ascii_uppercase()
                .as_str()
            {
                "USER_MODE" => return QueryMode::User,
                "SYSTEM_MODE" => return QueryMode::System,
                _ => (),
            }
        }
    }
    QueryMode::Other
}

fn access_mode(node: tree_sitter::Node<'_>, source: &str) -> Option<QueryMode> {
    fn path(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
        let node = unwrapped(node);
        match node.kind() {
            "identifier" => Some(node_text(Some(node), source)?.to_ascii_uppercase()),
            "field_access" => Some(format!(
                "{}.{}",
                path(node.child_by_field_name("object")?, source)?,
                path(node.child_by_field_name("field")?, source)?
            )),
            _ => None,
        }
    }
    match path(node, source)?.as_str() {
        "ACCESSLEVEL.USER_MODE" | "SYSTEM.ACCESSLEVEL.USER_MODE" => Some(QueryMode::User),
        "ACCESSLEVEL.SYSTEM_MODE" | "SYSTEM.ACCESSLEVEL.SYSTEM_MODE" => Some(QueryMode::System),
        _ => None,
    }
}

// Resolve only literal strings and literal concatenations. Unknown runtime text
// can already contain a mode; adding another one is not a safe transformation.
fn query_text(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let node = unwrapped(node);
    if node.kind() == "binary_expression"
        && node_text(node.child_by_field_name("operator"), source) == Some("+")
    {
        return Some(
            query_text(node.child_by_field_name("left")?, source)?
                + &query_text(node.child_by_field_name("right")?, source)?,
        );
    }
    if node.kind() != "string_literal" {
        return None;
    }
    let text = node_text(Some(node), source)?
        .strip_prefix('\'')?
        .strip_suffix('\'')?;
    let mut result = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        result.push(if c == '\\' {
            match chars.next()? {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '\'' => '\'',
                _ => return None,
            }
        } else {
            c
        });
    }
    Some(result)
}

fn literal_query_mode(text: &str) -> Option<QueryMode> {
    // Use the same Apex SOQL grammar as inline queries (including comments and binds).
    let prefix = "class QueryText { void run() { Object rows = [";
    let source = format!("{prefix}{text}]; }} }}");
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_sfapex::apex::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(&source, None)?;
    if tree.root_node().has_error() {
        return None;
    }
    fn body(
        node: tree_sitter::Node<'_>,
        start: usize,
        end: usize,
    ) -> Option<tree_sitter::Node<'_>> {
        if node.kind() == "query_expression" && node.start_byte() == start && node.end_byte() == end
        {
            return operands(node)
                .into_iter()
                .find(|child| child.kind() == "soql_query_body");
        }
        operands(node)
            .into_iter()
            .find_map(|child| body(child, start, end))
    }
    Some(query_mode(
        body(
            tree.root_node(),
            prefix.len() - 1,
            prefix.len() + text.len() + 1,
        )?,
        &source,
    ))
}

fn query_diagnostic(
    analysis: &mut Analysis,
    args: tree_sitter::Node<'_>,
    code: &'static str,
    message: &str,
) {
    analysis.diagnostics.push(Diagnostic {
        code, severity: Severity::Warning,
        start_byte: args.start_byte(), end_byte: args.end_byte(), message: message.to_owned(),
        suggestion: Some("Keep exactly one access mode for this query. Review its text and call together; do not blindly add SYSTEM_MODE.".to_owned()),
    });
}

fn collect_query_call(
    name: &str,
    args: tree_sitter::Node<'_>,
    source: &str,
    analysis: &mut Analysis,
) {
    let values = operands(args);
    let Some(first) = values.first().copied() else {
        return;
    };
    let first = unwrapped(first);
    let with_binds = name.to_ascii_lowercase().ends_with("withbinds");
    let base_arity = if with_binds { 2 } else { 1 };
    if values.len() < base_arity || values.len() > base_arity + 1 {
        return;
    }
    let explicit = values.get(base_arity).copied();
    let inline = first.kind() == "query_expression";
    let mode = if inline {
        operands(first)
            .first()
            .copied()
            .map(|body| query_mode(body, source))
    } else {
        query_text(first, source).and_then(|text| literal_query_mode(&text))
    };
    if let Some(argument) = explicit {
        if matches!(mode, Some(QueryMode::User | QueryMode::System)) {
            if !with_binds && access_mode(argument, source) == mode {
                // Remove only the comma and expression, preserving all comments.
                let comma = (0..args.child_count())
                    .filter_map(|i| args.child(i))
                    .find(|child| {
                        child.kind() == ","
                            && child.start_byte() >= values[base_arity - 1].end_byte()
                    });
                if let Some(comma) = comma {
                    analysis.edits.push(Edit {
                        start_byte: comma.start_byte(),
                        end_byte: comma.end_byte(),
                        replacement: String::new(),
                        rule_id: "duplicate-query-mode",
                    });
                    remove_code(argument, &mut analysis.edits);
                }
            } else {
                query_diagnostic(
                    analysis,
                    args,
                    "CONFLICTING_QUERY_MODES",
                    "The query and call both declare access modes; their policies cannot safely be merged automatically.",
                );
            }
        }
        return;
    }
    if inline {
        return;
    } // Static SOQL owns its mode; never append an argument too.
    match mode {
        Some(QueryMode::Missing) => append_access_level(
            args,
            if name.eq_ignore_ascii_case("getQueryLocator") {
                "database-get-query-locator"
            } else {
                "database-query"
            },
            &mut analysis.edits,
        ),
        Some(QueryMode::User | QueryMode::System) if !with_binds => (),
        Some(QueryMode::User | QueryMode::System) => query_diagnostic(
            analysis,
            args,
            "QUERY_MODE_REVIEW",
            "The query text declares a mode, but the WithBinds call requires an access-level argument. Review the call without adding a second mode.",
        ),
        Some(QueryMode::Other) => query_diagnostic(
            analysis,
            args,
            "UNSUPPORTED_SOQL_WITH",
            "The dynamic query has a different WITH clause; the call was left unchanged.",
        ),
        None => query_diagnostic(
            analysis,
            args,
            "DYNAMIC_QUERY_UNRESOLVED",
            "The query text cannot be verified statically; the call was left unchanged to avoid a duplicate or conflicting access mode.",
        ),
    }
}

fn remove_code(node: tree_sitter::Node<'_>, edits: &mut Vec<Edit>) {
    if node.is_extra() {
        return;
    }
    if node.child_count() == 0 {
        edits.push(Edit {
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            replacement: String::new(),
            rule_id: "duplicate-query-mode",
        });
    } else {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                remove_code(child, edits);
            }
        }
    }
}

fn database_access_level_rule(
    name: &str,
    arguments: tree_sitter::Node<'_>,
    source: &str,
) -> Option<&'static str> {
    let argument_count = operands(arguments).len();
    if argument_count == 0 || arguments_have_access_level(arguments, source) {
        return None;
    }

    // These are the dynamic counterparts of Apex's native DML statements.
    // The non-AccessLevel overloads have at most the following arities; adding
    // a trailing argument selects the corresponding AccessLevel overload.
    let is_immediate_or_async = [
        "insertAsync",
        "insertImmediate",
        "updateAsync",
        "updateImmediate",
        "deleteAsync",
        "deleteImmediate",
    ]
    .iter()
    .any(|operation| name.eq_ignore_ascii_case(operation));
    let max_non_access_level_arity =
        if name.eq_ignore_ascii_case("upsert") || name.eq_ignore_ascii_case("merge") {
            3
        } else if name.eq_ignore_ascii_case("convertLead") {
            2
        } else if [
            "insert",
            "update",
            "delete",
            "undelete",
            "insertAsync",
            "insertImmediate",
            "updateAsync",
            "updateImmediate",
            "deleteAsync",
            "deleteImmediate",
        ]
        .iter()
        .any(|operation| name.eq_ignore_ascii_case(operation))
        {
            if is_immediate_or_async { 1 } else { 2 }
        } else {
            return None;
        };

    (argument_count <= max_non_access_level_arity).then_some("database-dml")
}

fn arguments_have_access_level(arguments: tree_sitter::Node<'_>, source: &str) -> bool {
    operands(arguments)
        .into_iter()
        .any(|argument| access_mode(argument, source).is_some())
}

fn node_text<'a>(node: Option<tree_sitter::Node<'_>>, source: &'a str) -> Option<&'a str> {
    let node = node?;
    source.get(node.start_byte()..node.end_byte())
}

fn first_error(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    (0..node.child_count()).find_map(|i| first_error(node.child(i)?))
}

fn diagnostic(code: &'static str, start: usize, end: usize, message: &str) -> Diagnostic {
    Diagnostic {
        code,
        severity: Severity::Error,
        start_byte: start,
        end_byte: end,
        message: message.to_owned(),
        suggestion: Some("Fix the syntax and run systemeame again.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::analyze;

    fn assert_query_fix(query: &str, expected: &str) {
        let source =
            format!("public class Example {{ void run() {{ Object rows = [{query}]; }} }}");
        let analysis = analyze(&source);
        assert!(
            analysis.diagnostics.is_empty(),
            "{query}: {:?}",
            analysis.diagnostics
        );
        assert_eq!(analysis.edits.len(), 1, "{query}");
        assert_eq!(analysis.edits[0].rule_id, "static-soql");
        let fixed = crate::apply_edits(&source, &analysis.edits).unwrap();
        assert_eq!(fixed, source.replace(query, expected));
        let again = analyze(&fixed);
        assert!(
            again.diagnostics.is_empty(),
            "{fixed}: {:?}",
            again.diagnostics
        );
        assert!(again.edits.is_empty(), "{fixed}");
    }

    #[test]
    fn static_soql_places_mode_before_trailing_clauses() {
        for tail in [
            "",
            " ORDER BY Name DESC",
            " LIMIT 1",
            " LIMIT 10 OFFSET 2",
            " FOR UPDATE",
            " FOR VIEW",
            " UPDATE TRACKING",
            " ALL ROWS",
            " GROUP BY Name HAVING COUNT(Id) > 1 ORDER BY Name LIMIT 10",
        ] {
            for prefix in [
                "SELECT Id FROM Account",
                "SELECT Id FROM Account USING SCOPE mine",
                "SELECT Id FROM Account WHERE Id = :app.Account__r.PersonContactId",
            ] {
                assert_query_fix(
                    &format!("{prefix}{tail}"),
                    &format!("{prefix} WITH SYSTEM_MODE{tail}"),
                );
            }
        }
    }

    #[test]
    fn static_soql_preserves_comments_strings_unicode_and_crlf() {
        assert_query_fix(
            "select Id\r\nfrom Account\r\nwhere Name = 'José WITH USER_MODE' // keep\r\nORDER BY Name /* keep */\r\n",
            "select Id\r\nfrom Account\r\nwhere Name = 'José WITH USER_MODE' WITH SYSTEM_MODE // keep\r\nORDER BY Name /* keep */\r\n",
        );
        assert_query_fix(
            "SELECT Id FROM Account /* WITH USER_MODE */",
            "SELECT Id FROM Account WITH SYSTEM_MODE /* WITH USER_MODE */",
        );
    }

    #[test]
    fn static_soql_only_sets_mode_on_outer_query() {
        assert_query_fix(
            "SELECT Id, (SELECT Id FROM Contacts LIMIT 1) FROM Account WHERE Id IN (SELECT AccountId FROM Contact) LIMIT 1",
            "SELECT Id, (SELECT Id FROM Contacts LIMIT 1) FROM Account WHERE Id IN (SELECT AccountId FROM Contact) WITH SYSTEM_MODE LIMIT 1",
        );
    }

    #[test]
    fn static_soql_preserves_explicit_modes() {
        for mode in ["USER_MODE", "SYSTEM_MODE", "user_mode", "system_mode"] {
            let source = format!(
                "class Example {{ void run() {{ Object a = [SELECT Id, (SELECT Id FROM Contacts) FROM Account WITH /* keep */ {mode} LIMIT 1]; }} }}"
            );
            let analysis = analyze(&source);
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
            assert!(analysis.edits.is_empty());
        }
    }

    #[test]
    fn static_soql_reports_other_with_clauses_without_replacing_them() {
        for clause in [
            "SECURITY_ENFORCED",
            "DATA CATEGORY Geography__c ABOVE usa__c",
        ] {
            let source = format!(
                "class Example {{ void run() {{ Object a = [SELECT Id FROM Account WITH {clause}]; }} }}"
            );
            let analysis = analyze(&source);
            assert!(analysis.edits.is_empty());
            assert_eq!(analysis.diagnostics.len(), 1);
            assert_eq!(analysis.diagnostics[0].code, "UNSUPPORTED_SOQL_WITH");
        }
    }

    #[test]
    fn static_soql_ignores_apex_comments_and_query_strings() {
        let source = "class Example { void run() { /* [SELECT Id FROM Account] */ String q = '[SELECT Id FROM Account]'; } }";
        let analysis = analyze(source);
        assert!(analysis.diagnostics.is_empty());
        assert!(analysis.edits.is_empty());
    }

    #[test]
    fn static_soql_handles_loops_and_indexed_queries() {
        let source = "class Example { void run() { for (Account a : [SELECT Id FROM Account]) { } Id id = [SELECT Id FROM Account LIMIT 1][0].Id; } }";
        let analysis = analyze(source);
        assert!(analysis.diagnostics.is_empty());
        assert_eq!(analysis.edits.len(), 2);
        let fixed = crate::apply_edits(source, &analysis.edits).unwrap();
        assert!(fixed.contains("[SELECT Id FROM Account WITH SYSTEM_MODE]"));
        assert!(fixed.contains("[SELECT Id FROM Account WITH SYSTEM_MODE LIMIT 1][0].Id"));
        assert!(analyze(&fixed).diagnostics.is_empty());
        assert!(analyze(&fixed).edits.is_empty());
    }
    #[test]
    fn accepts_a_class_with_dml_and_query() {
        let source = "public class Example { void run(List<Account> rows) { insert rows; List<Account> a = [SELECT Id FROM Account]; } }";
        assert!(analyze(source).diagnostics.is_empty());
    }

    #[test]
    fn plans_native_dml_edit_without_touching_explicit_mode() {
        let source = "public class Example { void run(List<Account> rows) { insert rows; update as user rows; } }";
        let analysis = analyze(source);
        assert_eq!(analysis.edits.len(), 1);
        assert_eq!(analysis.edits[0].replacement, " as system");
        let fixed = crate::apply_edits(source, &analysis.edits).unwrap();
        assert!(fixed.contains("insert as system rows"));
        assert_eq!(analyze(&fixed).edits.len(), 0);
    }

    #[test]
    fn leaves_unknown_dynamic_text_for_review() {
        let source = "public class Example { void run(String query) { Database.query(query); } }";
        let analysis = analyze(source);
        assert!(analysis.edits.is_empty());
        assert_eq!(analysis.diagnostics[0].code, "DYNAMIC_QUERY_UNRESOLVED");
    }

    #[test]
    fn preserves_existing_database_query_modes() {
        let source = "public class Example { void run() { List<Account> a = Database.query('SELECT Id FROM Account WITH USER_MODE'); List<Account> b = Database.query('SELECT Id FROM Account', System.AccessLevel.SYSTEM_MODE); } }";
        assert!(analyze(source).edits.is_empty());
    }

    #[test]
    fn plans_access_levels_for_database_dml_and_query_locator() {
        let source = "public class Example { void run(List<Account> rows, Account master, Account duplicate) { Database.insert(rows); Database.update(rows, false); Database.upsert(rows, Account.External_Id__c, false); Database.delete(rows); Database.undelete(rows, true); Database.merge(master, duplicate); Database.getQueryLocator('SELECT Id FROM Account'); } }";
        let analysis = analyze(source);
        assert_eq!(analysis.edits.len(), 7, "{:?}", analysis.diagnostics);
        assert_eq!(
            analysis
                .edits
                .iter()
                .filter(|edit| edit.rule_id == "database-dml")
                .count(),
            6
        );
        assert_eq!(
            analysis
                .edits
                .iter()
                .filter(|edit| edit.rule_id == "database-get-query-locator")
                .count(),
            1
        );

        let fixed = crate::apply_edits(source, &analysis.edits).unwrap();
        for expected in [
            "Database.insert(rows, System.AccessLevel.SYSTEM_MODE)",
            "Database.update(rows, false, System.AccessLevel.SYSTEM_MODE)",
            "Database.upsert(rows, Account.External_Id__c, false, System.AccessLevel.SYSTEM_MODE)",
            "Database.delete(rows, System.AccessLevel.SYSTEM_MODE)",
            "Database.undelete(rows, true, System.AccessLevel.SYSTEM_MODE)",
            "Database.merge(master, duplicate, System.AccessLevel.SYSTEM_MODE)",
            "Database.getQueryLocator('SELECT Id FROM Account', System.AccessLevel.SYSTEM_MODE)",
        ] {
            assert!(fixed.contains(expected), "{fixed}");
        }
        assert!(analyze(&fixed).edits.is_empty(), "{fixed}");
    }

    #[test]
    fn preserves_existing_database_dml_and_query_locator_access_levels() {
        let source = "public class Example { void run(List<Account> rows) { Database.insert(rows, System.AccessLevel.USER_MODE); Database.update(rows, false, System.AccessLevel.SYSTEM_MODE); Database.upsert(rows, Account.External_Id__c, false, System.AccessLevel.USER_MODE); Database.getQueryLocator('SELECT Id FROM Account', System.AccessLevel.SYSTEM_MODE); Database.getQueryLocator('SELECT Id FROM Account WITH USER_MODE'); } }";
        assert!(analyze(source).diagnostics.is_empty());
        assert!(analyze(source).edits.is_empty());
    }

    #[test]
    fn plans_access_levels_for_other_supported_database_overloads() {
        let source = "public class Example { void run(List<Account> rows, Database.LeadConvert lead, Map<String, Object> binds) { Database.convertLead(lead, false); Database.insertImmediate(rows); Database.updateAsync(rows); Database.deleteImmediate(rows); Database.countQuery('SELECT COUNT() FROM Account'); Database.queryWithBinds('SELECT Id FROM Account', binds); Database.getQueryLocatorWithBinds('SELECT Id FROM Account', binds); } }";
        let analysis = analyze(source);
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        assert_eq!(analysis.edits.len(), 7);
        let fixed = crate::apply_edits(source, &analysis.edits).unwrap();
        for expected in [
            "Database.convertLead(lead, false, System.AccessLevel.SYSTEM_MODE)",
            "Database.insertImmediate(rows, System.AccessLevel.SYSTEM_MODE)",
            "Database.updateAsync(rows, System.AccessLevel.SYSTEM_MODE)",
            "Database.deleteImmediate(rows, System.AccessLevel.SYSTEM_MODE)",
            "Database.countQuery('SELECT COUNT() FROM Account', System.AccessLevel.SYSTEM_MODE)",
            "Database.queryWithBinds('SELECT Id FROM Account', binds, System.AccessLevel.SYSTEM_MODE)",
            "Database.getQueryLocatorWithBinds('SELECT Id FROM Account', binds, System.AccessLevel.SYSTEM_MODE)",
        ] {
            assert!(fixed.contains(expected), "{fixed}");
        }
        assert!(analyze(&fixed).edits.is_empty(), "{fixed}");
    }

    fn fix_call(call: &str) -> String {
        let source = format!("class Example {{ Object run() {{ return {call}; }} }}");
        let analysis = analyze(&source);
        assert!(
            analysis.diagnostics.is_empty(),
            "{call}: {:?}",
            analysis.diagnostics
        );
        let fixed = crate::apply_edits(&source, &analysis.edits).unwrap();
        let again = analyze(&fixed);
        assert!(
            again.diagnostics.is_empty(),
            "{fixed}: {:?}",
            again.diagnostics
        );
        assert!(again.edits.is_empty(), "not idempotent: {fixed}");
        fixed
    }

    #[test]
    fn inline_locator_has_one_mode_from_the_first_pass() {
        for query in [
            "[SELECT Id FROM Account]",
            "([SELECT Id FROM Account])",
            "[SELECT Id, (SELECT Id FROM Contacts) FROM Account WHERE Id IN (SELECT AccountId FROM Contact) LIMIT 1]",
            "[SELECT Id FROM Account WHERE Name = 'WITH USER_MODE' /* WITH SYSTEM_MODE */]",
        ] {
            let fixed = fix_call(&format!(
                "Database.getQueryLocator(/* before */ {query} /* after */)"
            ));
            assert!(!fixed.contains("AccessLevel"), "{fixed}");
            assert!(fixed.contains("WITH SYSTEM_MODE"), "{fixed}");
        }
    }

    #[test]
    fn inline_locator_preserves_either_mode_location_and_mode_expressions() {
        for mode in ["USER_MODE", "SYSTEM_MODE"] {
            let call = format!(
                "database.GETQUERYLOCATOR([SELECT Id FROM Account WITH /* keep */\n{mode}])"
            );
            assert!(fix_call(&call).contains(&call));
        }
        for mode in [
            "System.AccessLevel.SYSTEM_MODE",
            "AccessLevel.USER_MODE",
            "mode",
            "options.accessLevel",
            "flag ? AccessLevel.USER_MODE : AccessLevel.SYSTEM_MODE",
        ] {
            let call =
                format!("Database.getQueryLocator(([SELECT Id FROM Account]), /* keep */ {mode})");
            assert!(fix_call(&call).contains(&call));
        }
    }

    #[test]
    fn repairs_existing_duplicate_modes_without_losing_comments() {
        for mode in ["USER_MODE", "SYSTEM_MODE"] {
            for query in [
                format!("([SELECT Id FROM Account WITH /* inner */\n{mode}])"),
                format!("'SELECT Id FROM Account WITH\\n{mode}'"),
            ] {
                let fixed = fix_call(&format!(
                    "Database.getQueryLocator(\r\n{query}, /* before */ (system /* namespace */ . AccessLevel /* enum */ . {mode}) /* after */\r\n)"
                ));
                assert!(fixed.contains(&query));
                for comment in [
                    "/* before */",
                    "/* namespace */",
                    "/* enum */",
                    "/* after */",
                ] {
                    assert!(fixed.contains(comment), "{fixed}");
                }
                assert!(!fixed.contains("AccessLevel"), "{fixed}");
            }
        }
    }

    #[test]
    fn conflicting_and_unknown_duplicate_policies_require_review() {
        for query in [
            "[SELECT Id FROM Account WITH USER_MODE]",
            "'SELECT Id FROM Account WITH USER_MODE'",
        ] {
            for mode in ["System.AccessLevel.SYSTEM_MODE", "mode"] {
                let source = format!(
                    "class Example {{ void run() {{ Database.getQueryLocator({query}, {mode}); }} }}"
                );
                let analysis = analyze(&source);
                assert!(analysis.edits.is_empty());
                assert_eq!(analysis.diagnostics[0].code, "CONFLICTING_QUERY_MODES");
            }
        }
    }

    #[test]
    fn dynamic_queries_use_parsed_clauses_not_substring_matches() {
        for method in ["query", "countQuery", "getQueryLocator"] {
            for text in [
                "'SELECT Id FROM Account WITH\\nSYSTEM_MODE'",
                "'SELECT Id FROM Account WITH /* keep */ user_mode'",
                "('SELECT Id FROM Account WITH ' + 'SYSTEM_MODE')",
            ] {
                let call = format!("Database.{method}({text})");
                assert!(fix_call(&call).contains(&call));
                assert!(!fix_call(&call).contains("AccessLevel"));
            }
            for text in [
                "'SELECT Id FROM Account WHERE Name = \\'WITH USER_MODE\\''",
                "'SELECT Id FROM Account /* WITH SYSTEM_MODE */'",
                "'SELECT Id FROM Account' + ' ORDER BY Name'",
            ] {
                let fixed = fix_call(&format!("Database.{method}({text})"));
                assert!(
                    fixed.contains(&format!("{text}, System.AccessLevel.SYSTEM_MODE")),
                    "{fixed}"
                );
            }
        }
    }

    #[test]
    fn unknown_dynamic_queries_never_receive_a_second_hidden_mode() {
        for text in [
            "query",
            "buildQuery()",
            "'SELECT Id FROM Account ' + suffix",
            "flag ? 'SELECT Id FROM Account' : 'SELECT Id FROM Account WITH USER_MODE'",
        ] {
            for method in [
                "query",
                "countQuery",
                "getQueryLocator",
                "queryWithBinds",
                "countQueryWithBinds",
                "getQueryLocatorWithBinds",
            ] {
                let binds = if method.ends_with("WithBinds") {
                    ", binds"
                } else {
                    ""
                };
                let source = format!(
                    "class Example {{ void run() {{ Database.{method}({text}{binds}); }} }}"
                );
                let analysis = analyze(&source);
                assert!(analysis.edits.is_empty(), "{source}");
                assert_eq!(analysis.diagnostics[0].code, "DYNAMIC_QUERY_UNRESOLVED");
                let call = format!("Database.{method}({text}{binds}, mode)");
                assert!(fix_call(&call).contains(&call));
            }
        }
    }

    #[test]
    fn with_binds_never_gains_a_duplicate_or_loses_required_mode_argument() {
        for method in [
            "queryWithBinds",
            "countQueryWithBinds",
            "getQueryLocatorWithBinds",
        ] {
            for extra in ["", ", System.AccessLevel.SYSTEM_MODE"] {
                let source = format!(
                    "class Example {{ void run() {{ Database.{method}('SELECT Id FROM Account WITH SYSTEM_MODE', binds{extra}); }} }}"
                );
                let analysis = analyze(&source);
                assert!(analysis.edits.is_empty());
                assert_eq!(analysis.diagnostics.len(), 1);
            }
            let fixed = fix_call(&format!(
                "Database.{method}('SELECT Id FROM Account', /* keep */ binds /* tail */)"
            ));
            assert_eq!(fixed.matches("AccessLevel.SYSTEM_MODE").count(), 1);
        }
    }

    #[test]
    fn unrelated_nested_operations_do_not_inherit_an_outer_calls_mode() {
        let fixed = fix_call(
            "Database.query(buildQuery([SELECT Id FROM Account]), System.AccessLevel.USER_MODE)",
        );
        assert!(fixed.contains("[SELECT Id FROM Account WITH SYSTEM_MODE]"));
        let fixed = fix_call(
            "Database.getQueryLocator([SELECT Id FROM Account WHERE Id = :findId([SELECT Id FROM Contact])], System.AccessLevel.USER_MODE)",
        );
        assert!(fixed.contains("[SELECT Id FROM Contact WITH SYSTEM_MODE]"));
        assert!(!fixed.contains("Account WITH SYSTEM_MODE"));
    }

    #[test]
    fn dml_modes_are_structural_and_ambiguous_overloads_are_not_extended() {
        for call in [
            "Database.insert(rows, System.AccessLevel /* keep */ . SYSTEM_MODE)",
            "Database.update(rows, (AccessLevel.USER_MODE))",
            "Database.upsert(rows, Account.External_Id__c, false, AccessLevel.SYSTEM_MODE)",
        ] {
            assert!(fix_call(call).contains(call));
        }
        for call in [
            "Database.merge(master, duplicate, mode)",
            "Database.insert(rows, mode)",
            "Database.update(rows, false, mode)",
            "Database.upsert(rows, externalId)",
        ] {
            let source = format!("class Example {{ void run() {{ {call}; }} }}");
            let analysis = analyze(&source);
            assert!(analysis.edits.is_empty());
            // Full-arity calls are already explicit; shorter ambiguous overloads need review.
            if !call.contains("false, mode") {
                assert_eq!(analysis.diagnostics[0].code, "AMBIGUOUS_DML_ACCESS_LEVEL");
            }
        }
    }

    #[test]
    fn grammar_spike_records_node_shapes() {
        let source = "public class Example { void run(List<Account> rows) { insert rows; update as user rows; } }";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_sfapex::apex::LANGUAGE.into())
            .unwrap();
        let shape = parser.parse(source, None).unwrap().root_node().to_sexp();
        assert!(shape.contains("(dml_expression (dml_type (insert))"));
        assert!(shape.contains("security_mode: (dml_security_mode (user))"));
    }
    #[test]
    fn rejects_invalid_apex_without_edits() {
        let analysis = analyze("public class {");
        assert_eq!(analysis.diagnostics[0].code, "PARSE_ERROR");
        assert!(analysis.edits.is_empty());
    }
}
