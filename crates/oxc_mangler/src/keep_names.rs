use itertools::Itertools;
use oxc_ast::{AstKind, ast::*};
use oxc_semantic::{AstNode, AstNodes, ReferenceId, Scoping, SymbolId};
use rustc_hash::FxHashSet;

#[cfg_attr(not(test), expect(dead_code))]
pub fn collect_name_symbols(scoping: &Scoping, ast_nodes: &AstNodes) -> FxHashSet<SymbolId> {
    let collector = NameSymbolCollector::new(scoping, ast_nodes);
    collector.collect()
}

/// Collects symbols that are used to set `name` properties of functions and classes.
struct NameSymbolCollector<'a, 'b> {
    scoping: &'b Scoping,
    ast_nodes: &'b AstNodes<'a>,
}

impl<'a, 'b: 'a> NameSymbolCollector<'a, 'b> {
    fn new(scoping: &'b Scoping, ast_nodes: &'b AstNodes<'a>) -> Self {
        Self { scoping, ast_nodes }
    }

    fn collect(self) -> FxHashSet<SymbolId> {
        self.scoping
            .symbol_ids()
            .filter(|symbol_id| {
                let decl_node =
                    self.ast_nodes.get_node(self.scoping.symbol_declaration(*symbol_id));
                self.is_name_set_declare_node(decl_node, *symbol_id)
                    || self.has_name_set_reference_node(*symbol_id)
            })
            .collect()
    }

    fn has_name_set_reference_node(&self, symbol_id: SymbolId) -> bool {
        self.scoping.get_resolved_reference_ids(symbol_id).into_iter().any(|reference_id| {
            let node = self.ast_nodes.get_node(self.scoping.get_reference(*reference_id).node_id());
            self.is_name_set_reference_node(node, *reference_id)
        })
    }

    fn is_name_set_declare_node(&self, node: &'a AstNode, symbol_id: SymbolId) -> bool {
        match node.kind() {
            AstKind::Function(function) => {
                function.id.as_ref().is_some_and(|id| id.symbol_id() == symbol_id)
            }
            AstKind::Class(cls) => cls.id.as_ref().is_some_and(|id| id.symbol_id() == symbol_id),
            AstKind::VariableDeclarator(decl) => {
                if let BindingPatternKind::BindingIdentifier(id) = &decl.id.kind {
                    if id.symbol_id() == symbol_id {
                        return decl.init.as_ref().is_some_and(|init| {
                            self.is_expression_whose_name_needs_to_be_kept(init)
                        });
                    }
                }
                if let Some(assign_pattern) =
                    Self::find_assign_binding_pattern_kind_of_specific_symbol(
                        &decl.id.kind,
                        symbol_id,
                    )
                {
                    return self.is_expression_whose_name_needs_to_be_kept(&assign_pattern.right);
                }
                false
            }
            _ => false,
        }
    }

    fn is_name_set_reference_node(&self, node: &AstNode, reference_id: ReferenceId) -> bool {
        let Some(parent_node) = self.ast_nodes.parent_node(node.id()) else { return false };
        match parent_node.kind() {
            AstKind::SimpleAssignmentTarget(_) => {
                let Some((grand_parent_node_kind, grand_grand_parent_node_kind)) =
                    self.ast_nodes.ancestor_kinds(parent_node.id()).skip(1).take(2).collect_tuple()
                else {
                    return false;
                };
                debug_assert!(matches!(grand_parent_node_kind, AstKind::AssignmentTarget(_)));

                match grand_grand_parent_node_kind {
                    AstKind::AssignmentExpression(assign_expr) => {
                        Self::is_assignment_target_id_of_specific_reference(
                            &assign_expr.left,
                            reference_id,
                        ) && self.is_expression_whose_name_needs_to_be_kept(&assign_expr.right)
                    }
                    AstKind::AssignmentTargetWithDefault(assign_target) => {
                        Self::is_assignment_target_id_of_specific_reference(
                            &assign_target.binding,
                            reference_id,
                        ) && self.is_expression_whose_name_needs_to_be_kept(&assign_target.init)
                    }
                    _ => false,
                }
            }
            AstKind::ObjectAssignmentTarget(assign_target) => {
                assign_target.properties.iter().any(|property| {
                    if let AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(prop_id) =
                        &property
                    {
                        if prop_id.binding.reference_id() == reference_id {
                            return prop_id.init.as_ref().is_some_and(|init| {
                                self.is_expression_whose_name_needs_to_be_kept(init)
                            });
                        }
                    }
                    false
                })
            }
            _ => false,
        }
    }

    fn find_assign_binding_pattern_kind_of_specific_symbol(
        kind: &'a BindingPatternKind,
        symbol_id: SymbolId,
    ) -> Option<&'a AssignmentPattern<'a>> {
        match kind {
            BindingPatternKind::BindingIdentifier(_) => None,
            BindingPatternKind::ObjectPattern(object_pattern) => {
                for property in &object_pattern.properties {
                    if let Some(value) = Self::find_assign_binding_pattern_kind_of_specific_symbol(
                        &property.value.kind,
                        symbol_id,
                    ) {
                        return Some(value);
                    }
                }
                None
            }
            BindingPatternKind::ArrayPattern(array_pattern) => {
                for element in &array_pattern.elements {
                    let Some(binding) = element else { continue };

                    if let Some(value) = Self::find_assign_binding_pattern_kind_of_specific_symbol(
                        &binding.kind,
                        symbol_id,
                    ) {
                        return Some(value);
                    }
                }
                None
            }
            BindingPatternKind::AssignmentPattern(assign_pattern) => {
                if Self::is_binding_id_of_specific_symbol(&assign_pattern.left.kind, symbol_id) {
                    return Some(assign_pattern);
                }
                Self::find_assign_binding_pattern_kind_of_specific_symbol(
                    &assign_pattern.left.kind,
                    symbol_id,
                )
            }
        }
    }

    fn is_binding_id_of_specific_symbol(
        pattern_kind: &BindingPatternKind,
        symbol_id: SymbolId,
    ) -> bool {
        if let BindingPatternKind::BindingIdentifier(id) = pattern_kind {
            id.symbol_id() == symbol_id
        } else {
            false
        }
    }

    fn is_assignment_target_id_of_specific_reference(
        target_kind: &AssignmentTarget,
        reference_id: ReferenceId,
    ) -> bool {
        if let AssignmentTarget::AssignmentTargetIdentifier(id) = target_kind {
            id.reference_id() == reference_id
        } else {
            false
        }
    }

    #[expect(clippy::unused_self)]
    fn is_expression_whose_name_needs_to_be_kept(&self, expr: &Expression) -> bool {
        expr.is_anonymous_function_definition()
    }
}

#[cfg(test)]
mod test {
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use rustc_hash::FxHashSet;
    use std::iter::once;

    use super::collect_name_symbols;

    fn collect(source_text: &str) -> FxHashSet<String> {
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, source_text, SourceType::mjs()).parse();
        assert!(!ret.panicked, "{source_text}");
        assert!(ret.errors.is_empty(), "{source_text}");
        let ret = SemanticBuilder::new().build(&ret.program);
        assert!(ret.errors.is_empty(), "{source_text}");
        let semantic = ret.semantic;
        let symbols = collect_name_symbols(semantic.scoping(), semantic.nodes());
        symbols
            .into_iter()
            .map(|symbol_id| semantic.scoping().symbol_name(symbol_id).to_string())
            .collect()
    }

    #[test]
    fn test_declarations() {
        assert_eq!(collect("function foo() {}"), once("foo".to_string()).collect());
        assert_eq!(collect("class Foo {}"), once("Foo".to_string()).collect());
    }

    #[test]
    fn test_simple_declare_init() {
        assert_eq!(collect("var foo = function() {}"), once("foo".to_string()).collect());
        assert_eq!(collect("var foo = () => {}"), once("foo".to_string()).collect());
        assert_eq!(collect("var Foo = class {}"), once("Foo".to_string()).collect());
    }

    #[test]
    fn test_simple_assign() {
        assert_eq!(collect("var foo; foo = function() {}"), once("foo".to_string()).collect());
        assert_eq!(collect("var foo; foo = () => {}"), once("foo".to_string()).collect());
        assert_eq!(collect("var Foo; Foo = class {}"), once("Foo".to_string()).collect());

        assert_eq!(collect("var foo; foo ||= function() {}"), once("foo".to_string()).collect());
        assert_eq!(
            collect("var foo = 1; foo &&= function() {}"),
            once("foo".to_string()).collect()
        );
        assert_eq!(collect("var foo; foo ??= function() {}"), once("foo".to_string()).collect());
    }

    #[test]
    fn test_default_declarations() {
        assert_eq!(collect("var [foo = function() {}] = []"), once("foo".to_string()).collect());
        assert_eq!(collect("var [foo = () => {}] = []"), once("foo".to_string()).collect());
        assert_eq!(collect("var [Foo = class {}] = []"), once("Foo".to_string()).collect());
        assert_eq!(collect("var { foo = function() {} } = {}"), once("foo".to_string()).collect());
    }

    #[test]
    fn test_default_assign() {
        assert_eq!(
            collect("var foo; [foo = function() {}] = []"),
            once("foo".to_string()).collect()
        );
        assert_eq!(collect("var foo; [foo = () => {}] = []"), once("foo".to_string()).collect());
        assert_eq!(collect("var Foo; [Foo = class {}] = []"), once("Foo".to_string()).collect());
        assert_eq!(
            collect("var foo; ({ foo = function() {} } = {})"),
            once("foo".to_string()).collect()
        );
    }

    #[test]
    fn test_for_in_declaration() {
        assert_eq!(
            collect("for (var foo = function() {} in []) {}"),
            once("foo".to_string()).collect()
        );
        assert_eq!(collect("for (var foo = () => {} in []) {}"), once("foo".to_string()).collect());
        assert_eq!(collect("for (var Foo = class {} in []) {}"), once("Foo".to_string()).collect());
    }
}
