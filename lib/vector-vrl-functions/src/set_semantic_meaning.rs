use std::ops::{Deref, DerefMut};

use ::value::Value;
use lookup::LookupBuf;
use vrl::{diagnostic::Label, prelude::*};

#[derive(Debug, Default, Clone)]
pub struct MeaningList(pub BTreeMap<String, LookupBuf>);

impl Deref for MeaningList {
    type Target = BTreeMap<String, LookupBuf>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MeaningList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SetSemanticMeaning;

impl Function for SetSemanticMeaning {
    fn identifier(&self) -> &'static str {
        "set_semantic_meaning"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "target",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "meaning",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Sets custom field semantic meaning",
            source: r#"set_semantic_meaning(.foo, "bar")"#,
            result: Ok("null"),
        }]
    }

    fn compile(
        &self,
        (_, state): (&mut state::LocalEnv, &mut state::ExternalEnv),
        ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let span = ctx.span();
        let query = arguments.required_query("target")?;

        let meaning = arguments
            .required_literal("meaning")?
            .to_value()
            .try_bytes_utf8_lossy()
            .expect("meaning not bytes")
            .into_owned();

        // Semantic meaning can only be assigned to external fields.
        if !query.is_external() {
            let mut labels = vec![Label::primary(
                "the target of this semantic meaning is non-external",
                span,
            )];

            if let Some(variable) = query.as_variable() {
                labels.push(Label::context(
                    format!("maybe you meant \".{}\"?", variable.ident()),
                    span,
                ));
            }

            let error = ExpressionError::Error {
                message: "semantic meaning defined for non-external target".to_owned(),
                labels,
                notes: vec![],
            };

            return Err(Box::new(error) as Box<dyn DiagnosticMessage>);
        }

        let path = query.path().clone();

        let exists = state
            .target_kind()
            .map(|kind| {
                kind.find_at_path(&path.to_lookup())
                    .ok()
                    .flatten()
                    .is_some()
            })
            .unwrap_or_default();

        // Reject assigning meaning to non-existing field.
        if !exists {
            let error = ExpressionError::Error {
                message: "semantic meaning defined for non-existing field".to_owned(),
                labels: vec![
                    Label::primary("cannot assign semantic meaning to non-existing field", span),
                    Label::context(
                        format!("field \".{}\" is not known to exist for all events", &path),
                        span,
                    ),
                ],
                notes: vec![],
            };

            return Err(Box::new(error) as Box<dyn DiagnosticMessage>);
        }

        if let Some(list) = ctx.get_external_context_mut::<MeaningList>() {
            let duplicate = list.get(&meaning).filter(|&p| p != &path);

            // Disallow a single VRL program from assigning the same semantic meaning to two
            // different fields.
            if let Some(duplicate) = duplicate {
                let error = ExpressionError::Error {
                    message: "semantic meaning referencing two different fields".to_owned(),
                    labels: vec![
                        Label::primary(
                            format!(
                                "semantic meaning \"{}\" must reference a single field",
                                &meaning
                            ),
                            span,
                        ),
                        Label::context(
                            format!("already referencing field \".{}\"", &duplicate),
                            span,
                        ),
                    ],
                    notes: vec![],
                };

                return Err(Box::new(error) as Box<dyn DiagnosticMessage>);
            }

            list.insert(meaning, path);
        };

        Ok(Box::new(SetSemanticMeaningFn))
    }
}

#[derive(Debug, Clone)]
struct SetSemanticMeaningFn;

impl Expression for SetSemanticMeaningFn {
    fn resolve(&self, _ctx: &mut Context) -> Resolved {
        Ok(Value::Null)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }
}
