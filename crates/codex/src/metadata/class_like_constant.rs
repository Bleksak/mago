use serde::Deserialize;
use serde::Serialize;

use mago_interner::StringIdentifier;
use mago_span::HasSpan;
use mago_span::Span;

use crate::metadata::attribute::AttributeMetadata;
use crate::metadata::ttype::TypeMetadata;
use crate::ttype::atomic::TAtomic;
use crate::visibility::Visibility;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClassLikeConstantMetadata {
    pub attributes: Vec<AttributeMetadata>,
    pub name: StringIdentifier,
    pub span: Span,
    pub visibility: Visibility,
    pub type_declaration: Option<TypeMetadata>,
    pub type_metadata: Option<TypeMetadata>,
    pub inferred_type: Option<TAtomic>,
    pub is_final: bool,
    pub is_deprecated: bool,
    pub is_internal: bool,
}

impl ClassLikeConstantMetadata {
    pub fn new(name: StringIdentifier, span: Span, visibility: Visibility) -> Self {
        Self {
            attributes: Vec::new(),
            name,
            span,
            visibility,
            type_declaration: None,
            type_metadata: None,
            inferred_type: None,
            is_final: false,
            is_deprecated: false,
            is_internal: false,
        }
    }

    pub fn set_type_declaration(&mut self, type_declaration: TypeMetadata) {
        if self.type_metadata.is_none() {
            self.type_metadata = Some(type_declaration.clone());
        }

        self.type_declaration = Some(type_declaration);
    }
}

impl HasSpan for ClassLikeConstantMetadata {
    fn span(&self) -> Span {
        self.span
    }
}
