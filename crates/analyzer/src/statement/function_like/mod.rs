use std::rc::Rc;

use ahash::HashMap;

use mago_codex::context::ScopeContext;
use mago_codex::data_flow::graph::DataFlowGraph;
use mago_codex::data_flow::graph::GraphKind;
use mago_codex::data_flow::node::DataFlowNode;
use mago_codex::data_flow::node::DataFlowNodeId;
use mago_codex::data_flow::node::DataFlowNodeKind;
use mago_codex::data_flow::node::VariableSourceKind;
use mago_codex::data_flow::path::PathKind;
use mago_codex::get_class_like;
use mago_codex::get_interface;
use mago_codex::identifier::function_like::FunctionLikeIdentifier;
use mago_codex::identifier::method::MethodIdentifier;
use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::metadata::function_like::FunctionLikeMetadata;
use mago_codex::metadata::parameter::FunctionLikeParameterMetadata;
use mago_codex::ttype::TType;
use mago_codex::ttype::TypeRef;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::array::list::TList;
use mago_codex::ttype::atomic::callable::TCallable;
use mago_codex::ttype::atomic::generic::TGenericParameter;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::object::r#enum::TEnum;
use mago_codex::ttype::atomic::object::named::TNamedObject;
use mago_codex::ttype::atomic::reference::TReference;
use mago_codex::ttype::expander;
use mago_codex::ttype::expander::StaticClassType;
use mago_codex::ttype::expander::TypeExpansionOptions;
use mago_codex::ttype::get_mixed;
use mago_codex::ttype::get_mixed_any;
use mago_codex::ttype::union::TUnion;
use mago_codex::ttype::wrap_atomic;
use mago_codex::visibility::Visibility;
use mago_reporting::Annotation;
use mago_reporting::Issue;
use mago_span::HasSpan;
use mago_span::Span;
use mago_syntax::ast::*;

use crate::analyzable::Analyzable;
use crate::artifacts::AnalysisArtifacts;
use crate::context::Context;
use crate::context::block::BlockContext;
use crate::dataflow::unused_variables::check_variables_used;
use crate::error::AnalysisError;
use crate::issue::TypingIssueKind;
use crate::resolver::property::localize_property_type;
use crate::statement::analyze_statements;
use crate::statement::attributes::AttributeTarget;
use crate::statement::attributes::analyze_attributes;
use crate::utils::expression::get_variable_id;

use super::r#return::handle_return_value;

pub mod function;

#[derive(Debug, Clone, Copy)]
pub enum FunctionLikeBody<'a> {
    Statements(&'a [Statement]),
    Expression(&'a Expression),
}

pub fn analyze_function_like<'a, 'ast>(
    context: &mut Context<'a>,
    parent_artifacts: &mut AnalysisArtifacts,
    scope: ScopeContext<'a>,
    function_like_metadata: &'a FunctionLikeMetadata,
    parameter_list: &'ast FunctionLikeParameterList,
    body: FunctionLikeBody<'ast>,
    import_variables: HashMap<String, Rc<TUnion>>,
) -> Result<(BlockContext<'a>, AnalysisArtifacts), AnalysisError> {
    let mut previous_type_resolution_context = std::mem::replace(
        &mut context.type_resolution_context,
        function_like_metadata.type_resolution_context.clone().unwrap_or_default(),
    );

    let mut block_context = BlockContext::new(scope);
    let mut artifacts = AnalysisArtifacts::new(DataFlowGraph::new(context.settings.graph_kind));
    artifacts.type_variable_bounds = parent_artifacts.type_variable_bounds.clone();

    add_parameter_types_to_context(
        context,
        &mut block_context,
        &mut artifacts,
        function_like_metadata,
        parameter_list,
    )?;

    for (variable_name, variable_type) in import_variables {
        block_context.possibly_assigned_variable_ids.insert(variable_name.clone());
        block_context.locals.insert(variable_name, variable_type);
    }

    if !scope.is_static()
        && let Some(class_like_metadata) = scope.get_class_like()
    {
        let mut this_type = wrap_atomic(TAtomic::Object(get_this_type(context, class_like_metadata)));

        if function_like_metadata.kind.is_method()
            && let Some(method_name) = &function_like_metadata.name
            && let GraphKind::WholeProgram = &artifacts.data_flow_graph.kind
            && class_like_metadata.specialized_instance
        {
            let new_call_node = DataFlowNode::get_for_this_before_method(
                MethodIdentifier::new(class_like_metadata.original_name, *method_name),
                function_like_metadata.return_type_metadata.as_ref().map(|type_metadata| type_metadata.span),
                None,
            );

            this_type.parent_nodes = vec![new_call_node];
        }

        block_context.locals.insert("$this".to_string(), Rc::new(this_type));
    }

    if let FunctionLikeBody::Statements(statements) = body {
        for statement in statements {
            let Statement::Global(global) = statement else {
                if statement.is_noop() {
                    continue;
                } else {
                    break;
                }
            };

            for variable in global.variables.iter() {
                if let Some(var_id) = get_variable_id(variable, context.interner) {
                    block_context.conditionally_referenced_variable_ids.insert(var_id);
                }
            }
        }
    }

    if let Some(calling_class) = block_context.scope.get_class_like_name()
        && let Some(class_like_metadata) = get_class_like(context.codebase, context.interner, calling_class)
    {
        add_properties_to_context(context, &mut block_context, class_like_metadata, function_like_metadata)?;
    }

    if !function_like_metadata.unchecked {
        match body {
            FunctionLikeBody::Statements(statements) => {
                analyze_statements(statements, context, &mut block_context, &mut artifacts)?;
            }
            FunctionLikeBody::Expression(value) => {
                block_context.inside_return = true;
                value.analyze(context, &mut block_context, &mut artifacts)?;
                block_context.inside_return = false;
                block_context.conditionally_referenced_variable_ids = Default::default();

                let value_type = artifacts.get_expression_type(value).cloned().unwrap_or_else(get_mixed_any);

                handle_return_value(
                    context,
                    &mut block_context,
                    &mut artifacts,
                    Some(value),
                    value_type,
                    value.span(),
                )?;
            }
        }

        if !block_context.has_returned {
            handle_reference_at_return(context, &mut block_context, &mut artifacts, function_like_metadata);
        }

        if let GraphKind::WholeProgram = &artifacts.data_flow_graph.kind
            && let Some(method_metadata) = &function_like_metadata.method_metadata
            && !method_metadata.is_static
            && let Some(this_type) = block_context.locals.get("$this")
        {
            let calling_class = block_context
                .scope
                .get_class_like_name()
                .expect("Expected the calling class to be present in the context");

            let method_name =
                function_like_metadata.name.expect("Expected the function like metadata to contain a method name");

            let new_call_node = DataFlowNode::get_for_this_after_method(
                MethodIdentifier::new(*calling_class, method_name),
                function_like_metadata.name_span,
                None,
            );

            for parent_node in &this_type.parent_nodes {
                artifacts.data_flow_graph.add_path(parent_node, &new_call_node, PathKind::Default);
            }

            artifacts.data_flow_graph.add_node(new_call_node);
        }

        if context.settings.find_unused_expressions {
            report_unused_variables(context, &block_context, &mut artifacts, function_like_metadata);
        }
    }

    std::mem::swap(&mut context.type_resolution_context, &mut previous_type_resolution_context);
    for (expression_range, expression_type) in std::mem::take(&mut artifacts.expression_types) {
        parent_artifacts.expression_types.insert(expression_range, expression_type);
    }

    Ok((block_context, artifacts))
}

fn add_parameter_types_to_context<'a>(
    context: &mut Context<'a>,
    block_context: &mut BlockContext<'a>,
    artifacts: &mut AnalysisArtifacts,
    function_like_metadata: &FunctionLikeMetadata,
    parameter_list: &FunctionLikeParameterList,
) -> Result<(), AnalysisError> {
    for (i, parameter_metadata) in function_like_metadata.parameters.iter().enumerate() {
        let mut parameter_type = if let Some(type_signature) = parameter_metadata.get_type_metadata() {
            add_symbol_references(
                &type_signature.type_union,
                block_context.scope.get_function_like_identifier().as_ref(),
                artifacts,
            );

            let signature_union = type_signature.type_union.clone();

            if !signature_union.is_mixed() {
                let mut parameter_type = signature_union.clone();
                let calling_class = block_context.scope.get_class_like_name();

                expander::expand_union(
                    context.codebase,
                    context.interner,
                    &mut parameter_type,
                    &TypeExpansionOptions {
                        self_class: calling_class,
                        static_class_type: if let Some(calling_class) = calling_class {
                            StaticClassType::Name(*calling_class)
                        } else {
                            StaticClassType::None
                        },
                        evaluate_class_constants: true,
                        evaluate_conditional_types: true,
                        function_is_final: if let Some(method_metadata) = &function_like_metadata.method_metadata {
                            method_metadata.is_final
                        } else {
                            false
                        },
                        expand_generic: true,
                        expand_templates: true,
                        file_path: Some(&context.source.identifier),
                        ..Default::default()
                    },
                );

                for type_node in parameter_type.get_all_child_nodes() {
                    if let TypeRef::Atomic(TAtomic::Reference(TReference::Symbol { name, .. })) = type_node {
                        context.buffer.report(
                            TypingIssueKind::NonExistentClassLike,
                            Issue::error(format!(
                                "Class or interface or enum `{}` not found",
                                context.interner.lookup(name)
                            ))
                            .with_annotation(Annotation::primary(type_signature.span).with_message(
                                "This type contains a reference to a class or interface or enum that does not exist.",
                            ))
                            .with_note("Ensure that the class or interface or enum is defined and imported correctly.")
                            .with_help("If this is a class or interface or enum that is defined in another file, ensure that the file is included or autoloaded correctly."),
                        );
                    }
                }

                parameter_type
            } else {
                signature_union
            }
        } else {
            get_mixed_any()
        };

        let parameter_node = if let Some(parameter_node) = parameter_list.parameters.get(i) {
            parameter_node
        } else {
            continue;
        };

        analyze_attributes(
            context,
            block_context,
            artifacts,
            parameter_node.attribute_lists.as_slice(),
            if parameter_node.is_promoted_property() {
                AttributeTarget::PromotedProperty
            } else {
                AttributeTarget::Parameter
            },
        )?;

        if let Some(default_value) = parameter_node.default_value.as_ref() {
            default_value.value.analyze(context, block_context, artifacts)?;
        }

        if parameter_metadata.is_variadic() {
            parameter_type = wrap_atomic(TAtomic::Array(TArray::List(TList::new(Box::new(parameter_type)))));
        }

        let new_parent_node = if let GraphKind::WholeProgram = &artifacts.data_flow_graph.kind {
            DataFlowNode::get_for_lvar(*parameter_metadata.get_name(), parameter_metadata.get_name_span())
        } else {
            DataFlowNode {
                id: DataFlowNodeId::Parameter(*parameter_metadata.get_name(), parameter_metadata.get_span()),
                kind: DataFlowNodeKind::VariableUseSource {
                    span: parameter_metadata.get_name_span(),
                    kind: if parameter_metadata.is_by_reference() {
                        VariableSourceKind::RefParameter
                    } else if block_context.calling_closure_id.is_some() {
                        VariableSourceKind::ClosureParameter
                    } else if let Some(method_metadata) = &function_like_metadata.method_metadata {
                        match method_metadata.visibility {
                            Visibility::Public | Visibility::Protected => VariableSourceKind::NonPrivateParameter,
                            Visibility::Private => VariableSourceKind::PrivateParameter,
                        }
                    } else {
                        VariableSourceKind::PrivateParameter
                    },
                    pure: false,
                    has_parent_nodes: true,
                    from_loop_init: false,
                },
            }
        };

        artifacts.data_flow_graph.add_node(new_parent_node.clone());

        if let GraphKind::WholeProgram = &artifacts.data_flow_graph.kind {
            let calling_id = if let Some(calling_closure_id) = block_context.calling_closure_id {
                FunctionLikeIdentifier::Closure(calling_closure_id)
            } else {
                block_context
                    .scope
                    .get_function_like_identifier()
                    .expect("Expected the calling function id to be present")
            };

            let argument_node =
                DataFlowNode::get_for_method_argument(calling_id, i, Some(parameter_metadata.get_name_span()), None);

            artifacts.data_flow_graph.add_path(&argument_node, &new_parent_node, PathKind::Default);
            artifacts.data_flow_graph.add_node(argument_node);
        }

        parameter_type.parent_nodes.push(new_parent_node);

        block_context
            .locals
            .insert(context.interner.lookup(&parameter_metadata.get_name().0).to_string(), Rc::new(parameter_type));
    }

    Ok(())
}

fn add_properties_to_context<'a>(
    context: &Context<'a>,
    block_context: &mut BlockContext<'a>,
    class_like_metadata: &ClassLikeMetadata,
    function_like_metadata: &FunctionLikeMetadata,
) -> Result<(), AnalysisError> {
    for (property_name_id, declaring_class) in &class_like_metadata.declaring_property_ids {
        let Some(property_class_metadata) = get_class_like(context.codebase, context.interner, declaring_class) else {
            return Err(AnalysisError::InternalError(
                format!("Could not load property class metadata for `{}`.", context.interner.lookup(declaring_class)),
                class_like_metadata.span,
            ));
        };

        let Some(property_metadata) = property_class_metadata.properties.get(property_name_id) else {
            return Err(AnalysisError::InternalError(
                format!("Could not load property metadata for `{}`.", context.interner.lookup(property_name_id)),
                class_like_metadata.span,
            ));
        };

        let mut property_type = property_metadata
            .type_metadata
            .as_ref()
            .map(|type_metadata| &type_metadata.type_union)
            .cloned()
            .unwrap_or_else(get_mixed);

        let property_name = context.interner.lookup(property_name_id);
        let raw_property_name = property_name.strip_prefix("$").unwrap_or(property_name);

        let expression_id = if property_metadata.is_static() {
            format!("{}::${raw_property_name}", context.interner.lookup(&class_like_metadata.name),)
        } else {
            let this_type = get_this_type(context, class_like_metadata);

            property_type = localize_property_type(
                context,
                &property_type,
                this_type.get_type_parameters().unwrap_or_default(),
                class_like_metadata,
                property_class_metadata,
            );

            format!("$this->{raw_property_name}")
        };

        let calling_class = block_context.scope.get_class_like_name();

        expander::expand_union(
            context.codebase,
            context.interner,
            &mut property_type,
            &TypeExpansionOptions {
                self_class: calling_class,
                static_class_type: StaticClassType::Name(*calling_class.unwrap()),
                function_is_final: if let Some(method_metadata) = &function_like_metadata.method_metadata {
                    method_metadata.is_final
                } else {
                    false
                },
                expand_generic: true,
                file_path: Some(&context.source.identifier),
                ..Default::default()
            },
        );

        block_context.locals.insert(expression_id, Rc::new(property_type));
    }

    Ok(())
}

fn get_this_type(context: &Context<'_>, class_like_metadata: &ClassLikeMetadata) -> TObject {
    if class_like_metadata.kind.is_enum() {
        return TObject::Enum(TEnum { name: class_like_metadata.original_name, case: None });
    }

    let mut intersections = vec![];
    for required_interface in &class_like_metadata.require_implements {
        let Some(interface_metadata) = get_interface(context.codebase, context.interner, required_interface) else {
            continue;
        };

        let TObject::Named(mut interface_type) = get_this_type(context, interface_metadata) else {
            continue;
        };

        let interface_intersactions = std::mem::take(&mut interface_type.intersection_types);

        interface_type.is_this = false;
        intersections.push(TAtomic::Object(TObject::Named(interface_type)));
        if let Some(interface_intersactions) = interface_intersactions {
            intersections.extend(interface_intersactions);
        }
    }

    for required_class in &class_like_metadata.require_extends {
        let Some(parent_class_metadata) = get_class_like(context.codebase, context.interner, required_class) else {
            continue;
        };

        let TObject::Named(mut parent_type) = get_this_type(context, parent_class_metadata) else {
            continue;
        };

        let parent_intersections = std::mem::take(&mut parent_type.intersection_types);

        parent_type.is_this = false;
        intersections.push(TAtomic::Object(TObject::Named(parent_type)));
        if let Some(parent_intersections) = parent_intersections {
            intersections.extend(parent_intersections);
        }
    }

    TObject::Named(TNamedObject {
        name: class_like_metadata.original_name,
        type_parameters: if !class_like_metadata.template_types.is_empty() {
            Some(
                class_like_metadata
                    .template_types
                    .iter()
                    .map(|(parameter_name, template_map)| {
                        let (defining_entry, constraint) = template_map.iter().next().unwrap();

                        wrap_atomic(TAtomic::GenericParameter(TGenericParameter {
                            parameter_name: *parameter_name,
                            constraint: Box::new(constraint.clone()),
                            defining_entity: *defining_entry,
                            intersection_types: None,
                        }))
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        },
        is_this: true,
        intersection_types: if intersections.is_empty() { None } else { Some(intersections) },
        remapped_parameters: false,
    })
}

fn handle_reference_at_return<'a>(
    context: &Context<'a>,
    block_context: &mut BlockContext<'a>,
    artifacts: &mut AnalysisArtifacts,
    function_like_metadata: &FunctionLikeMetadata,
) {
    for (i, parameter) in function_like_metadata.parameters.iter().enumerate() {
        if !parameter.is_by_reference() {
            continue;
        }

        let Some(context_type) = block_context.locals.get(context.interner.lookup(&parameter.get_name().0)) else {
            continue;
        };

        let new_parent_node = if let GraphKind::WholeProgram = &artifacts.data_flow_graph.kind {
            DataFlowNode::get_for_method_argument_reference(
                block_context.scope.get_function_like_identifier().unwrap(),
                i,
                Some(parameter.get_name_span()),
                None,
            )
        } else {
            DataFlowNode::get_for_unlabelled_sink(parameter.get_name_span())
        };

        artifacts.data_flow_graph.add_node(new_parent_node.clone());

        for parent_node in &context_type.parent_nodes {
            artifacts.data_flow_graph.add_path(parent_node, &new_parent_node, PathKind::Default);
        }
    }
}

fn add_symbol_references(
    parameter_type: &TUnion,
    calling_function_like_id: Option<&FunctionLikeIdentifier>,
    artifacts: &mut AnalysisArtifacts,
) {
    for type_node in parameter_type.get_all_child_nodes() {
        if let TypeRef::Atomic(atomic) = type_node {
            match atomic {
                TAtomic::Reference(TReference::Symbol { name, .. })
                | TAtomic::Callable(TCallable::Alias(FunctionLikeIdentifier::Function(name))) => {
                    match calling_function_like_id {
                        Some(FunctionLikeIdentifier::Function(calling_function)) => {
                            artifacts.symbol_references.add_symbol_reference_to_symbol(*calling_function, *name, true);
                        }
                        Some(FunctionLikeIdentifier::Method(calling_classlike, calling_function)) => {
                            artifacts.symbol_references.add_class_member_reference_to_symbol(
                                (*calling_classlike, *calling_function),
                                *name,
                                true,
                            );
                        }
                        _ => {}
                    }
                }
                TAtomic::Callable(TCallable::Alias(FunctionLikeIdentifier::Method(name, member_name))) => {
                    match calling_function_like_id {
                        Some(FunctionLikeIdentifier::Function(calling_function)) => {
                            artifacts.symbol_references.add_symbol_reference_to_class_member(
                                *calling_function,
                                (*name, *member_name),
                                true,
                            );
                        }
                        Some(FunctionLikeIdentifier::Method(calling_classlike, calling_function)) => {
                            artifacts.symbol_references.add_class_member_reference_to_class_member(
                                (*calling_classlike, *calling_function),
                                (*name, *member_name),
                                true,
                            );
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

fn report_unused_variables<'a>(
    context: &mut Context<'a>,
    block_context: &BlockContext<'a>,
    artifacts: &mut AnalysisArtifacts,
    function_like_metadata: &FunctionLikeMetadata,
) {
    if !context.settings.find_unused_variables {
        return;
    }

    let unused_source_nodes = check_variables_used(&artifacts.data_flow_graph);

    for node in &unused_source_nodes.0 {
        match &node.kind {
            DataFlowNodeKind::VariableUseSource { kind, span, pure, .. } => {
                if let VariableSourceKind::Default = &kind {
                    handle_unused_assignment(context, block_context, span, node, artifacts, pure);
                }
            }
            _ => {
                // This should not happen, but if it does, we can skip it.
                continue;
            }
        };
    }

    for node in &unused_source_nodes.1 {
        match &node.kind {
            DataFlowNodeKind::VariableUseSource { kind, span, .. } => {
                if let DataFlowNodeId::Var(var_id, ..) | DataFlowNodeId::Parameter(var_id, ..) = &node.id
                    && context.interner.lookup(&var_id.0).starts_with("$_")
                {
                    continue;
                }

                match &kind {
                    VariableSourceKind::PrivateParameter => {
                        let Some(parameter) = get_parameter_from_node(function_like_metadata, &node.id) else {
                            continue;
                        };

                        if parameter.is_promoted_property {
                            // Do not report unused parameters that are promoted properties.
                            continue;
                        }

                        let label = node.id.to_label(context.interner);

                        context.buffer.report(
                            TypingIssueKind::UnusedParameter,
                            Issue::help(format!("Parameter `{label}` is never used."))
                                .with_annotation(
                                    Annotation::primary(parameter.span).with_message("Parameter declared here is never used."),
                                )
                                .with_note(
                                    "Unused parameters can indicate dead code or an opportunity for refactoring.",
                                )
                                .with_help(format!(
                                    "Remove the parameter `{label}`, use it within the function, or prefix it with `$_` if it is intentionally unused."
                                )),
                        );
                    }
                    VariableSourceKind::ClosureParameter => {
                        let Some(span) = get_parameter_span(function_like_metadata, &node.id) else {
                            continue;
                        };

                        let label = node.id.to_label(context.interner);

                        context.buffer.report(
                            TypingIssueKind::UnusedClosureParameter,
                            Issue::help(format!(
                                "Variable `{label}` is never used in this closure."
                            ))
                            .with_annotation(
                                Annotation::primary(span)
                                    .with_message("This variable is imported into the closure but never used.")
                            )
                            .with_note(
                                "Unused closure used variables can indicate dead code or a potential logic error within the closure."
                            )
                            .with_help(
                                format!("Remove the closure variable `{label}`, or use it within the closure.")
                            ),
                        );
                    }
                    VariableSourceKind::ClosureUse => {
                        let label = node.id.to_label(context.interner);
                        context.buffer.report(
                            TypingIssueKind::UnusedClosureUse,
                            Issue::help(format!("Closure variable `{label}` is never used."))
                                .with_annotation(
                                    Annotation::primary(*span).with_message("This closure variable is never used."),
                                )
                                .with_note(
                                    "Unused closure variables can indicate dead code or a potential logic error within the closure.",
                                )
                                .with_help(format!(
                                    "Remove the closure variable `{label}`, or use it within the closure."
                                )),
                        );
                    }
                    VariableSourceKind::NonPrivateParameter => {
                        // todo register public/private param
                    }
                    VariableSourceKind::Default => {
                        handle_unused_assignment(context, block_context, span, node, artifacts, &false);
                    }
                    VariableSourceKind::RefParameter => {
                        // do nothing
                    }
                }
            }
            _ => {
                panic!()
            }
        };
    }
}

fn handle_unused_assignment(
    context: &mut Context<'_>,
    block_context: &BlockContext<'_>,
    span: &Span,
    node: &DataFlowNode,
    artifacts: &AnalysisArtifacts,
    pure: &bool,
) {
    if let DataFlowNodeId::Var(var_id, ..) | DataFlowNodeId::Parameter(var_id, ..) = &node.id {
        let var_str = context.interner.lookup(&var_id.0);
        if var_str.starts_with("$_") {
            // Skip unused variables that are prefixed with $_
            return;
        }

        if block_context.static_locals.contains(var_str) {
            // Skip static locals
            return;
        }
    }

    let unused_closure_variable = artifacts
        .closure_spans
        .iter()
        .any(|closure_span| span.start.offset > closure_span.0 && span.start.offset < closure_span.1);

    let label = node.id.to_label(context.interner);

    if unused_closure_variable {
        context.buffer.report(
            TypingIssueKind::UnusedAssignmentInClosure,
            Issue::help(format!("Assignment to `{label}` is unused within this closure."))
                .with_code(TypingIssueKind::UnusedAssignmentInClosure)
                .with_annotation(
                    Annotation::primary(*span)
                        .with_message("This assignment is never used locally.")
                )
                .with_note(
                    "Variables assigned inside a closure but not used locally might indicate dead code or a potential logic error."
                )
                .with_help(
                    format!("Remove the assignment to `{label}`, or use it within the closure.")
                ),
        );
    } else if *pure {
        context.buffer.report(
            TypingIssueKind::UnusedAssignmentStatement,
            Issue::help(format!(
                "Assignment to `{label}` is unused and the expression has no side effects."
            ))
            .with_code(TypingIssueKind::UnusedAssignmentStatement)
            .with_annotation(Annotation::primary(*span).with_message("Unused assignment with no side effects."))
            .with_note(
                "The value assigned is never read, and the assignment operation itself is pure (has no other effects).",
            )
            .with_help("Remove this entire assignment statement as it has no effect."),
        );
    } else {
        context.buffer.report(
            TypingIssueKind::UnusedAssignment,
            Issue::help(format!("Assignment to `{label}` is unused."))
                .with_code(TypingIssueKind::UnusedAssignment)
                .with_annotation(
                    Annotation::primary(*span)
                        .with_message("The value assigned here is never used.")
                )
                .with_note(
                    "Although the assigned value is never read, the assignment expression itself might have side effects."
                )
                .with_help(
                    format!("Consider removing the assignment part (`{label} = `) if only the side effects of the right-hand side are needed, or remove the entire statement if neither the value nor the side effects are required.")
                ),
        );
    }
}

fn get_parameter_span(metadata: &FunctionLikeMetadata, id: &DataFlowNodeId) -> Option<Span> {
    get_parameter_from_node(metadata, id).map(|parameter| parameter.get_span())
}

fn get_parameter_from_node<'a>(
    metadata: &'a FunctionLikeMetadata,
    id: &DataFlowNodeId,
) -> Option<&'a FunctionLikeParameterMetadata> {
    if let DataFlowNodeId::Parameter(variable_id, ..) = id {
        metadata.parameters.iter().find(|p| p.get_name().0 == variable_id.0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use crate::test_analysis;

    test_analysis! {
        name = properties_added_to_context,
        code = indoc! {r#"
            <?php

            namespace DateTime {
                final class Duration
                {
                    public function isPositive(): bool
                    {
                        return true;
                    }
                }

                /**
                 * @consistent-constructor
                 */
                abstract class AbstractTemporal
                {
                    public static function monotonic(): static
                    {
                        return new static();
                    }
                }

                final class Timestamp extends AbstractTemporal
                {
                    public function plus(Duration $_duration): static
                    {
                        return new static();
                    }

                    public function since(Timestamp $_timestamp): Duration
                    {
                        return new Duration();
                    }
                }
            }

            namespace Example {
                use Closure;
                use DateTime\Duration;
                use DateTime\Timestamp;

                final class OptionalIncrementalTimeout
                {
                    /**
                     * @var ?Timestamp The end time.
                     */
                    private null|Timestamp $end;

                    /**
                     * @var (Closure(): ?Duration) The handler to be called upon timeout.
                     */
                    private Closure $handler;

                    /**
                     * @param null|Duration $timeout The timeout duration. Null to disable timeout.
                     * @param (Closure(): ?Duration) $handler The handler to be executed if the timeout is reached.
                     */
                    public function __construct(null|Duration $timeout, Closure $handler)
                    {
                        $this->handler = $handler;

                        if (null === $timeout) {
                            $this->end = null;

                            return;
                        }

                        if (!$timeout->isPositive()) {
                            $this->end = Timestamp::monotonic();
                            return;
                        }

                        $this->end = Timestamp::monotonic()->plus($timeout);
                    }

                    /**
                     * Retrieves the remaining time until the timeout is reached, or null if no timeout is set.
                     *
                     * If the timeout has already been exceeded, the handler is invoked, and its return value is provided.
                     *
                     * @return Duration|null The remaining time duration, null if no timeout is set, or the handler's return value if the timeout is exceeded.
                     */
                    public function getRemaining(): null|Duration
                    {
                        if ($this->end === null) {
                            return null;
                        }

                        $remaining = $this->end->since(Timestamp::monotonic());

                        return $remaining->isPositive() ? $remaining : ($this->handler)();
                    }
                }
            }
        "#},
    }
}
