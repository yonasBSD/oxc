//! Generator of code related to `AstKind`.
//!
//! * `AstType` type definition.
//! * `AstKind` type definition.
//! * `AstKind::ty` method.
//! * `AstKind::as_*` methods.
//! * `GetSpan` impl for `AstKind`.
//!
//! Variants of `AstKind` and `AstType` are not created for types listed in `BLACK_LIST` below.

use quote::{format_ident, quote};

use crate::{
    AST_CRATE_PATH, Codegen, Generator,
    output::{Output, output_path},
    schema::{Def, Schema, TypeDef},
    utils::number_lit,
};

use super::define_generator;

/// Types to omit creating an `AstKind` for.
///
/// Apart from this list every type with `#[ast(visit)]` attr gets an `AstKind`.
const BLACK_LIST: [&str; 61] = [
    "Span",
    "Expression",
    "ObjectPropertyKind",
    "TemplateElement",
    "ComputedMemberExpression",
    "StaticMemberExpression",
    "PrivateFieldExpression",
    "AssignmentTargetRest",
    "AssignmentTargetMaybeDefault",
    "AssignmentTargetProperty",
    "AssignmentTargetPropertyIdentifier",
    "AssignmentTargetPropertyProperty",
    "ChainElement",
    "Statement",
    "Declaration",
    "ForStatementLeft",
    "BindingPattern",
    "BindingPatternKind",
    "BindingProperty",
    "ClassElement",
    "AccessorProperty",
    "ImportDeclarationSpecifier",
    "WithClause",
    "ImportAttribute",
    "ImportAttributeKey",
    "ExportDefaultDeclarationKind",
    "ModuleExportName",
    "JSXOpeningFragment",
    "JSXClosingFragment",
    "TSEnumMemberName",
    "TSLiteral",
    "TSType",
    "TSTypeOperator",
    "TSArrayType",
    "TSTupleType",
    "TSOptionalType",
    "TSRestType",
    "TSTupleElement",
    "TSInterfaceBody",
    "TSSignature",
    "TSIndexSignature",
    "TSCallSignatureDeclaration",
    "TSIndexSignatureName",
    "TSTypePredicate",
    "TSTypePredicateName",
    "TSModuleDeclarationName",
    "TSModuleDeclarationBody",
    "TSTypeQueryExprName",
    "TSFunctionType",
    "TSConstructorType",
    "TSNamespaceExportDeclaration",
    "JSDocNullableType",
    "JSDocNonNullableType",
    "JSDocUnknownType",
    "JSXExpression",
    "JSXEmptyExpression",
    "JSXAttribute",
    "JSXAttributeName",
    "JSXAttributeValue",
    "JSXChild",
    "JSXSpreadChild",
];

/// Generator for `AstKind`, `AstType`, and related code.
pub struct AstKindGenerator;

define_generator!(AstKindGenerator);

impl Generator for AstKindGenerator {
    /// Set `has_kind` for structs and enums which are visited, and not on blacklist.
    fn prepare(&self, schema: &mut Schema, _codegen: &Codegen) {
        // Set `has_kind` to `true` for all visited types
        for type_def in &mut schema.types {
            match type_def {
                TypeDef::Struct(struct_def) => {
                    struct_def.kind.has_kind = struct_def.visit.has_visitor();
                }
                TypeDef::Enum(enum_def) => {
                    enum_def.kind.has_kind = enum_def.visit.has_visitor();
                }
                _ => {}
            }
        }

        // Set `has_kind` to `false` for types on blacklist
        for type_name in BLACK_LIST {
            let type_def = schema.type_by_name_mut(type_name);
            match type_def {
                TypeDef::Struct(struct_def) => struct_def.kind.has_kind = false,
                TypeDef::Enum(enum_def) => enum_def.kind.has_kind = false,
                _ => panic!(
                    "Type which is not a struct or enum on `AstKind` blacklist: `{}`",
                    type_def.name()
                ),
            }
        }
    }

    /// Generate `AstKind` etc definitions.
    fn generate(&self, schema: &Schema, _codegen: &Codegen) -> Output {
        let mut type_variants = quote!();
        let mut kind_variants = quote!();
        let mut span_match_arms = quote!();
        let mut as_methods = quote!();

        let mut next_index = 0u16;
        for type_def in &schema.types {
            let has_kind = match type_def {
                TypeDef::Struct(struct_def) => struct_def.kind.has_kind,
                TypeDef::Enum(enum_def) => enum_def.kind.has_kind,
                _ => false,
            };
            if !has_kind {
                continue;
            }

            let type_ident = type_def.ident();
            let type_ty = type_def.ty(schema);

            assert!(u8::try_from(next_index).is_ok());
            let index = number_lit(next_index);
            type_variants.extend(quote!( #type_ident = #index, ));
            kind_variants.extend(quote!( #type_ident(&'a #type_ty) = AstType::#type_ident as u8, ));

            span_match_arms.extend(quote!( Self::#type_ident(it) => it.span(), ));

            let as_method_name = format_ident!("as_{}", type_def.snake_name());
            as_methods.extend(quote! {
                ///@@line_break
                #[inline]
                pub fn #as_method_name(self) -> Option<&'a #type_ty> {
                    if let Self::#type_ident(v) = self {
                        Some(v)
                    } else {
                        None
                    }
                }
            });

            next_index += 1;
        }

        let output = quote! {
            #![expect(missing_docs)] ///@ FIXME (in ast_tools/src/generators/ast_kind.rs)

            ///@@line_break
            use std::ptr;

            ///@@line_break
            use oxc_span::{GetSpan, Span};

            ///@@line_break
            use crate::ast::*;

            ///@@line_break
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            #[repr(u8)]
            pub enum AstType {
                #type_variants
            }

            ///@@line_break
            /// Untyped AST Node Kind
            #[derive(Debug, Clone, Copy)]
            #[repr(C, u8)]
            pub enum AstKind<'a> {
                #kind_variants
            }

            ///@@line_break
            impl AstKind<'_> {
                /// Get the [`AstType`] of an [`AstKind`].
                #[inline]
                pub fn ty(&self) -> AstType {
                    ///@ SAFETY: `AstKind` is `#[repr(C, u8)]`, so discriminant is stored in first byte,
                    ///@ and it's valid to read it.
                    ///@ `AstType` is also `#[repr(u8)]` and `AstKind` and `AstType` both have the same
                    ///@ discriminants, so it's valid to read `AstKind`'s discriminant as `AstType`.
                    unsafe { *ptr::from_ref(self).cast::<AstType>().as_ref().unwrap_unchecked() }
                }
            }

            ///@@line_break
            impl GetSpan for AstKind<'_> {
                fn span(&self) -> Span {
                    match self {
                        #span_match_arms
                    }
                }
            }

            ///@@line_break
            impl<'a> AstKind<'a> {
                #as_methods
            }
        };

        Output::Rust { path: output_path(AST_CRATE_PATH, "ast_kind.rs"), tokens: output }
    }
}
