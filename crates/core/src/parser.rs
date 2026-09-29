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
    collect_database_query(root, source, &mut analysis.edits);
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
        } else if let Some(anchor) = node
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

/// Adds an explicit AccessLevel to the one-argument `Database.query` overload.
///
/// The call shape is identified by grammar fields, never by a source-wide text
/// search. A one-argument dynamic query is deliberately included: callers have
/// chosen an explicit system-execution policy for dynamic query text. Direct
/// query literals that already declare a user/system clause are left alone.
fn collect_database_query(node: tree_sitter::Node<'_>, source: &str, edits: &mut Vec<Edit>) {
    if node.kind() == "method_invocation"
        && node_text(node.child_by_field_name("object"), source)
            .is_some_and(|object| object.eq_ignore_ascii_case("Database"))
        && node_text(node.child_by_field_name("name"), source)
            .is_some_and(|name| name.eq_ignore_ascii_case("query"))
        && let Some(arguments) = node.child_by_field_name("arguments")
        && arguments.named_child_count() == 1
        && !argument_declares_query_mode(arguments.named_child(0), source)
    {
        // `argument_list` spans the delimiters; this appends to the existing
        // argument without changing any source bytes inside the query text.
        edits.push(Edit {
            start_byte: arguments.end_byte() - 1,
            end_byte: arguments.end_byte() - 1,
            replacement: ", System.AccessLevel.SYSTEM_MODE".to_owned(),
            rule_id: "database-query",
        });
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            collect_database_query(child, source, edits);
        }
    }
}

fn node_text<'a>(node: Option<tree_sitter::Node<'_>>, source: &'a str) -> Option<&'a str> {
    let node = node?;
    source.get(node.start_byte()..node.end_byte())
}

fn argument_declares_query_mode(argument: Option<tree_sitter::Node<'_>>, source: &str) -> bool {
    let Some(text) = node_text(argument, source) else {
        return false;
    };
    let upper = text.to_ascii_uppercase();
    upper.contains("WITH USER_MODE") || upper.contains("WITH SYSTEM_MODE")
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
    fn plans_database_query_mode_for_dynamic_query_variable() {
        let source = "public class Example { void run() { String query = 'SELECT Id FROM Account'; List<Account> rows = Database.query(query); } }";
        let analysis = analyze(source);
        assert_eq!(analysis.edits.len(), 1);
        assert_eq!(analysis.edits[0].rule_id, "database-query");
        let fixed = crate::apply_edits(source, &analysis.edits).unwrap();
        assert!(fixed.contains("Database.query(query, System.AccessLevel.SYSTEM_MODE)"));
        assert!(analyze(&fixed).edits.is_empty());
    }

    #[test]
    fn preserves_existing_database_query_modes() {
        let source = "public class Example { void run() { List<Account> a = Database.query('SELECT Id FROM Account WITH USER_MODE'); List<Account> b = Database.query('SELECT Id FROM Account', System.AccessLevel.SYSTEM_MODE); } }";
        assert!(analyze(source).edits.is_empty());
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
