// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The Parley shaping/projection path is narrowly adapted from DioxusLabs/blitz
// commit d788124ab881f9bb537cb452ec1d837604a374a8:
// - packages/blitz-dom/src/node/text.rs
// - packages/blitz-paint/src/text.rs

use std::{collections::BTreeMap, sync::Arc};

use parley::{
    FontContext, FontFamily, FontFamilyName, LayoutContext, TextStyle,
    fontique::{
        Attributes, Blob, Collection, CollectionOptions, FontInfoOverride, FontStyle, FontWeight,
        FontWidth, QueryFamily, QueryStatus,
    },
};
use thiserror::Error;

use crate::stylo_to_parley::TextBrush;
use crate::system_fonts::SystemFontFamilyResolver;

pub(crate) struct ParleyDocumentServices {
    pub(crate) font_context: FontContext,
    pub(crate) layout_context: LayoutContext<TextBrush>,
    system_font_family_resolver: Option<SystemFontFamilyResolver>,
    inline_font_metrics_cache: Vec<(
        TextStyle<'static, 'static, TextBrush>,
        Option<InlineFontMetrics>,
    )>,
}

/// Primary-font metrics attached to one resolved Parley text style.
///
/// A shaped run may use a fallback font for its glyphs. CSSOM text geometry,
/// like Blink's `FragmentItem`, instead uses the primary font metrics of the
/// style that owns the run.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InlineFontMetrics {
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) line_height: f32,
    pub(crate) x_height: f32,
}

fn resolved_inline_x_height(ascent: f32, x_height: Option<f32>) -> f32 {
    x_height.unwrap_or(ascent * 0.56).max(0.0)
}

impl ParleyDocumentServices {
    fn clear_inline_font_metrics_cache(&mut self) {
        self.inline_font_metrics_cache.clear();
    }

    pub(crate) fn resolve_system_font_families(
        &mut self,
        style: &mut TextStyle<'static, 'static, TextBrush>,
    ) {
        let Some(resolver) = self.system_font_family_resolver.as_mut() else {
            return;
        };
        resolver.resolve_text_style(&mut self.font_context.collection, style);
    }

    pub(crate) fn inline_font_metrics(
        &mut self,
        style: &TextStyle<'static, 'static, TextBrush>,
        sample: Option<char>,
    ) -> Option<InlineFontMetrics> {
        if let Some((_, metrics)) = self
            .inline_font_metrics_cache
            .iter()
            .find(|(cached, _)| cached == style)
        {
            // A font such as Baidu's icon font may not contain `x`. A later
            // call carrying one of the style's real characters must be able
            // to retry a previous sample-free miss.
            if metrics.is_some() || sample.is_none() {
                return *metrics;
            }
        }

        // Shape a character from the selected primary face rather than
        // borrowing metrics from an arbitrary fallback run. Most fonts cover
        // `x`; icon fonts often do not, so the owning text contributes one
        // additional candidate and the font identity verifies the result.
        let primary_font = self.primary_font_identity(style);
        let metrics = ['x'].into_iter().chain(sample).find_map(|candidate| {
            let candidate = candidate.to_string();
            let mut builder = self.layout_context.style_run_builder(
                &mut self.font_context,
                &candidate,
                1.0,
                true,
            );
            let style_index = builder.push_style(style.clone());
            builder.push_style_run(style_index, ..);
            let mut layout = builder.build(&candidate);
            layout.break_all_lines(None);
            let run = layout.lines().next()?.runs().next()?;
            if primary_font
                .is_some_and(|identity| identity != (run.font().data.id(), run.font().index))
            {
                return None;
            }
            let metrics = *run.metrics();
            Some(InlineFontMetrics {
                ascent: metrics.ascent,
                descent: metrics.descent,
                line_height: metrics.line_height,
                x_height: resolved_inline_x_height(metrics.ascent, metrics.x_height),
            })
        });
        if let Some((_, cached)) = self
            .inline_font_metrics_cache
            .iter_mut()
            .find(|(cached, _)| cached == style)
        {
            *cached = metrics;
        } else {
            self.inline_font_metrics_cache
                .push((style.clone(), metrics));
        }
        metrics
    }

    fn primary_font_identity(
        &mut self,
        style: &TextStyle<'static, 'static, TextBrush>,
    ) -> Option<(u64, u32)> {
        let parsed_source;
        let families = match &style.font_family {
            FontFamily::Single(family) => vec![family],
            FontFamily::List(families) => families.iter().collect(),
            FontFamily::Source(source) => {
                parsed_source = FontFamilyName::parse_css_list(source)
                    .filter_map(Result::ok)
                    .collect::<Vec<_>>();
                parsed_source.iter().collect()
            }
        };
        let query_families = families.iter().map(|family| match family {
            FontFamilyName::Named(name) => QueryFamily::Named(name),
            FontFamilyName::Generic(family) => QueryFamily::Generic(*family),
        });
        let FontContext {
            collection,
            source_cache,
        } = &mut self.font_context;
        let mut query = collection.query(source_cache);
        query.set_families(query_families);
        query.set_attributes(Attributes::new(
            style.font_width,
            style.font_style,
            style.font_weight,
        ));
        let mut identity = None;
        query.matches_with(|font| {
            identity = Some((font.blob.id(), font.index));
            QueryStatus::Stop
        });
        identity
    }
}

/// Whether a document's font collection may discover platform fonts.
///
/// Tests and deterministic differential runners disable this and register a
/// fixed web font set. Product documents enable it by default so CSS generic
/// families still have platform fallback when no downloadable font covers a
/// character.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SystemFontPolicy {
    /// Discover fonts through Fontique's platform backend.
    #[default]
    Enabled,
    /// Restrict shaping to explicitly registered web fonts.
    Disabled,
}

impl SystemFontPolicy {
    const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// CSS `font-style` metadata attached to one downloadable font face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WebFontStyle {
    Normal,
    Italic,
    Oblique(Option<f32>),
}

impl WebFontStyle {
    fn to_fontique(self) -> FontStyle {
        match self {
            Self::Normal => FontStyle::Normal,
            Self::Italic => FontStyle::Italic,
            Self::Oblique(angle) => FontStyle::Oblique(angle),
        }
    }
}

/// Fontique metadata overrides derived from a CSS `@font-face` rule.
///
/// `weight` uses the CSS numeric range and `stretch` is a percentage where
/// `100.0` is normal width. Missing descriptors leave the font's own metadata
/// intact.
#[derive(Clone, Debug, PartialEq)]
pub struct WebFontFace {
    family_name: String,
    weight: Option<f32>,
    stretch: Option<f32>,
    style: Option<WebFontStyle>,
}

impl WebFontFace {
    pub fn new(family_name: impl Into<String>) -> Self {
        Self {
            family_name: family_name.into(),
            weight: None,
            stretch: None,
            style: None,
        }
    }

    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn with_stretch(mut self, percentage: f32) -> Self {
        self.stretch = Some(percentage);
        self
    }

    pub fn with_style(mut self, style: WebFontStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn family_name(&self) -> &str {
        &self.family_name
    }

    pub const fn weight(&self) -> Option<f32> {
        self.weight
    }

    pub const fn stretch(&self) -> Option<f32> {
        self.stretch
    }

    pub const fn style(&self) -> Option<WebFontStyle> {
        self.style
    }

    fn validate(&self) -> Result<(), WebFontRegistrationError> {
        if self.family_name.trim().is_empty() {
            return Err(WebFontRegistrationError::InvalidDescriptor {
                detail: "font-family must not be empty".to_owned(),
            });
        }
        if self
            .weight
            .is_some_and(|value| !value.is_finite() || !(1.0..=1000.0).contains(&value))
        {
            return Err(WebFontRegistrationError::InvalidDescriptor {
                detail: "font-weight must be a finite value from 1 through 1000".to_owned(),
            });
        }
        if self
            .stretch
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            return Err(WebFontRegistrationError::InvalidDescriptor {
                detail: "font-stretch must be a positive finite percentage".to_owned(),
            });
        }
        if self.style.is_some_and(
            |style| matches!(style, WebFontStyle::Oblique(Some(angle)) if !angle.is_finite()),
        ) {
            return Err(WebFontRegistrationError::InvalidDescriptor {
                detail: "font-style oblique angle must be finite".to_owned(),
            });
        }
        Ok(())
    }

    fn fontique_override(&self) -> FontInfoOverride<'_> {
        FontInfoOverride {
            family_name: Some(&self.family_name),
            width: self.stretch.map(FontWidth::from_percentage),
            style: self.style.map(WebFontStyle::to_fontique),
            weight: self.weight.map(FontWeight::new),
            axes: None,
        }
    }
}

/// One owner-validated downloadable font response.
///
/// `slot` is chosen by the stylesheet/resource owner. Reusing it replaces the
/// old face atomically after the new payload has decoded and validated.
#[derive(Clone, Debug, PartialEq)]
pub struct WebFontRegistration {
    slot: String,
    face: WebFontFace,
    bytes: Vec<u8>,
}

impl WebFontRegistration {
    pub fn new(slot: impl Into<String>, face: WebFontFace, bytes: Vec<u8>) -> Self {
        Self {
            slot: slot.into(),
            face,
            bytes,
        }
    }

    pub fn slot(&self) -> &str {
        &self.slot
    }

    pub fn face(&self) -> &WebFontFace {
        &self.face
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebFontRegistrationOutcome {
    Added,
    Replaced,
    Unchanged,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WebFontRegistrationError {
    #[error("web font slot must not be empty")]
    EmptySlot,
    #[error("invalid web font descriptor: {detail}")]
    InvalidDescriptor { detail: String },
    #[error("failed to decode {format} web font")]
    DecodeFailed { format: &'static str },
    #[error("web font payload contains no supported OpenType font")]
    UnsupportedPayload,
}

#[derive(Clone, Debug, PartialEq)]
struct RegisteredWebFont {
    face: WebFontFace,
    sfnt_bytes: Arc<[u8]>,
}

/// Lazily initialized text resources reused by successive layout demands for
/// one committed Document.
///
/// The renderer owns this sidecar. A one-shot [`LayoutWorld`] only borrows the
/// contexts while building pass-local Parley layouts; neither context escapes
/// in [`crate::PaintSnapshot`].
pub struct DocumentLayoutServices {
    // FontContext and LayoutContext are both large. Keep them off the stack so
    // embedding this sidecar in ScriptVm does not inflate every VM frame.
    parley: Option<Box<ParleyDocumentServices>>,
    system_font_policy: SystemFontPolicy,
    web_fonts: BTreeMap<String, RegisteredWebFont>,
    pub(crate) text_layout_passes: u64,
}

impl Default for DocumentLayoutServices {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentLayoutServices {
    /// Creates an uninitialized document sidecar.
    pub const fn new() -> Self {
        Self {
            parley: None,
            system_font_policy: SystemFontPolicy::Enabled,
            web_fonts: BTreeMap::new(),
            text_layout_passes: 0,
        }
    }

    /// Creates a document sidecar with an explicit platform-font policy.
    pub const fn with_system_font_policy(system_font_policy: SystemFontPolicy) -> Self {
        Self {
            parley: None,
            system_font_policy,
            web_fonts: BTreeMap::new(),
            text_layout_passes: 0,
        }
    }

    pub const fn system_font_policy(&self) -> SystemFontPolicy {
        self.system_font_policy
    }

    /// Returns whether a demand with non-empty text has initialized Parley.
    pub const fn is_initialized(&self) -> bool {
        self.parley.is_some()
    }

    /// Counts text-bearing one-shot passes served by these reused contexts.
    pub const fn text_layout_passes(&self) -> u64 {
        self.text_layout_passes
    }

    pub fn web_font_count(&self) -> usize {
        self.web_fonts.len()
    }

    pub(crate) fn begin_inline_layout_pass(&mut self) {
        if let Some(parley) = self.parley.as_deref_mut() {
            parley.clear_inline_font_metrics_cache();
        }
    }

    /// Adds or replaces one owner-validated font face.
    ///
    /// This API performs no document/generation check: the resource owner must
    /// call it only after matching its stable document, rule/slot, and request
    /// identity. Invalid new bytes leave an existing slot untouched.
    pub fn register_web_font(
        &mut self,
        registration: WebFontRegistration,
    ) -> Result<WebFontRegistrationOutcome, WebFontRegistrationError> {
        if registration.slot.trim().is_empty() {
            return Err(WebFontRegistrationError::EmptySlot);
        }
        registration.face.validate()?;
        let sfnt_bytes = decode_web_font_bytes(&registration.bytes)?;
        validate_registered_font(&registration.face, Arc::clone(&sfnt_bytes))?;
        let font = RegisteredWebFont {
            face: registration.face,
            sfnt_bytes,
        };
        let outcome = match self.web_fonts.get(&registration.slot) {
            Some(current) if current == &font => WebFontRegistrationOutcome::Unchanged,
            Some(_) => WebFontRegistrationOutcome::Replaced,
            None => WebFontRegistrationOutcome::Added,
        };
        if outcome == WebFontRegistrationOutcome::Unchanged {
            return Ok(outcome);
        }
        self.web_fonts.insert(registration.slot, font);
        if self.parley.is_some() {
            self.parley = Some(Box::new(build_parley_services(
                self.system_font_policy,
                &self.web_fonts,
            )));
        }
        Ok(outcome)
    }

    /// Removes a font slot. Returns whether a registered face was removed.
    pub fn remove_web_font(&mut self, slot: &str) -> bool {
        if self.web_fonts.remove(slot).is_none() {
            return false;
        }
        if self.parley.is_some() {
            self.parley = Some(Box::new(build_parley_services(
                self.system_font_policy,
                &self.web_fonts,
            )));
        }
        true
    }

    pub(crate) fn parley_mut(&mut self) -> &mut ParleyDocumentServices {
        if self.parley.is_none() {
            self.parley = Some(Box::new(build_parley_services(
                self.system_font_policy,
                &self.web_fonts,
            )));
        }
        self.parley.as_deref_mut().expect("Parley was initialized")
    }
}

fn build_parley_services(
    system_font_policy: SystemFontPolicy,
    web_fonts: &BTreeMap<String, RegisteredWebFont>,
) -> ParleyDocumentServices {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: system_font_policy.is_enabled(),
    });
    let system_font_family_resolver = system_font_policy
        .is_enabled()
        .then(|| SystemFontFamilyResolver::new(&mut collection));
    let mut font_context = FontContext {
        collection,
        source_cache: Default::default(),
    };
    for font in web_fonts.values() {
        register_font(&mut font_context, font);
    }
    ParleyDocumentServices {
        font_context,
        layout_context: LayoutContext::new(),
        system_font_family_resolver,
        inline_font_metrics_cache: Vec::new(),
    }
}

fn register_font(font_context: &mut FontContext, font: &RegisteredWebFont) -> bool {
    let data: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(Arc::clone(&font.sfnt_bytes));
    !font_context
        .collection
        .register_fonts(Blob::new(data), Some(font.face.fontique_override()))
        .is_empty()
}

fn validate_registered_font(
    face: &WebFontFace,
    sfnt_bytes: Arc<[u8]>,
) -> Result<(), WebFontRegistrationError> {
    let mut font_context = FontContext {
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
        source_cache: Default::default(),
    };
    let font = RegisteredWebFont {
        face: face.clone(),
        sfnt_bytes,
    };
    register_font(&mut font_context, &font)
        .then_some(())
        .ok_or(WebFontRegistrationError::UnsupportedPayload)
}

fn decode_web_font_bytes(bytes: &[u8]) -> Result<Arc<[u8]>, WebFontRegistrationError> {
    let decoded = match bytes.get(..4) {
        Some(b"wOFF") => wuff::decompress_woff1(bytes)
            .map_err(|_| WebFontRegistrationError::DecodeFailed { format: "WOFF" })?,
        Some(b"wOF2") => wuff::decompress_woff2(bytes)
            .map_err(|_| WebFontRegistrationError::DecodeFailed { format: "WOFF2" })?,
        _ => bytes.to_vec(),
    };
    Ok(Arc::from(decoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TTF: &[u8] = include_bytes!("../tests/fixtures/moli-ahem.ttf");
    const TEST_CJK_TTF: &[u8] = include_bytes!("../tests/fixtures/moli-cjk.ttf");
    const TEST_WOFF: &[u8] = include_bytes!("../tests/fixtures/moli-ahem.woff");
    const TEST_WOFF2: &[u8] = include_bytes!("../tests/fixtures/moli-ahem.woff2");

    #[test]
    fn missing_x_height_uses_the_blink_ascent_fallback() {
        assert!((resolved_inline_x_height(10.0, None) - 5.6).abs() < f32::EPSILON);
        assert_eq!(resolved_inline_x_height(10.0, Some(4.25)), 4.25);
    }

    #[test]
    fn primary_metrics_retry_with_an_owning_character_when_x_is_missing() {
        let mut services =
            DocumentLayoutServices::with_system_font_policy(SystemFontPolicy::Disabled);
        services
            .register_web_font(WebFontRegistration::new(
                "cjk-face",
                WebFontFace::new("Moli CJK"),
                TEST_CJK_TTF.to_vec(),
            ))
            .unwrap();
        services
            .register_web_font(WebFontRegistration::new(
                "latin-face",
                WebFontFace::new("Moli Latin"),
                TEST_TTF.to_vec(),
            ))
            .unwrap();
        let style = TextStyle {
            font_family: FontFamily::List(std::borrow::Cow::Owned(vec![
                FontFamilyName::Named(std::borrow::Cow::Borrowed("Moli CJK")),
                FontFamilyName::Named(std::borrow::Cow::Borrowed("Moli Latin")),
            ])),
            ..TextStyle::default()
        };
        let parley = services.parley_mut();

        assert!(parley.inline_font_metrics(&style, None).is_none());
        assert!(
            parley.inline_font_metrics(&style, Some('中')).is_some(),
            "the style's actual glyph should recover metrics from a primary font without x"
        );
    }

    fn registration(slot: &str, family: &str, bytes: &[u8]) -> WebFontRegistration {
        WebFontRegistration::new(
            slot,
            WebFontFace::new(family)
                .with_weight(625.0)
                .with_stretch(87.5)
                .with_style(WebFontStyle::Italic),
            bytes.to_vec(),
        )
    }

    fn has_family(services: &mut DocumentLayoutServices, family: &str) -> bool {
        services
            .parley_mut()
            .font_context
            .collection
            .family_id(family)
            .is_some()
    }

    #[test]
    fn fixed_font_policy_registers_ttf_under_css_alias() {
        let mut services =
            DocumentLayoutServices::with_system_font_policy(SystemFontPolicy::Disabled);
        assert_eq!(services.system_font_policy(), SystemFontPolicy::Disabled);
        assert!(!services.is_initialized());

        assert_eq!(
            services.register_web_font(registration("face-1", "Phase Three Alias", TEST_TTF)),
            Ok(WebFontRegistrationOutcome::Added)
        );
        assert_eq!(services.web_font_count(), 1);
        assert!(!services.is_initialized());
        assert!(has_family(&mut services, "Phase Three Alias"));
    }

    #[test]
    fn woff_and_woff2_are_decoded_before_fontique_registration() {
        for (slot, family, bytes) in [
            ("woff", "Moli WOFF", TEST_WOFF),
            ("woff2", "Moli WOFF2", TEST_WOFF2),
        ] {
            let mut services =
                DocumentLayoutServices::with_system_font_policy(SystemFontPolicy::Disabled);
            assert_eq!(
                services.register_web_font(registration(slot, family, bytes)),
                Ok(WebFontRegistrationOutcome::Added),
                "{slot} should decode and register"
            );
            assert!(has_family(&mut services, family));
        }
    }

    #[test]
    fn stable_slot_replacement_rebuilds_initialized_font_collection() {
        let mut services =
            DocumentLayoutServices::with_system_font_policy(SystemFontPolicy::Disabled);
        services
            .register_web_font(registration("rule-7", "Old Alias", TEST_TTF))
            .unwrap();
        assert!(has_family(&mut services, "Old Alias"));

        assert_eq!(
            services.register_web_font(registration("rule-7", "New Alias", TEST_WOFF2)),
            Ok(WebFontRegistrationOutcome::Replaced)
        );
        assert!(!has_family(&mut services, "Old Alias"));
        assert!(has_family(&mut services, "New Alias"));
        assert_eq!(services.web_font_count(), 1);
    }

    #[test]
    fn invalid_replacement_does_not_poison_existing_slot() {
        let mut services =
            DocumentLayoutServices::with_system_font_policy(SystemFontPolicy::Disabled);
        services
            .register_web_font(registration("rule-9", "Stable Alias", TEST_TTF))
            .unwrap();
        assert!(has_family(&mut services, "Stable Alias"));

        assert_eq!(
            services.register_web_font(registration("rule-9", "Broken Alias", b"not a font")),
            Err(WebFontRegistrationError::UnsupportedPayload)
        );
        assert!(has_family(&mut services, "Stable Alias"));
        assert!(!has_family(&mut services, "Broken Alias"));
        assert_eq!(services.web_font_count(), 1);
    }

    #[test]
    fn unchanged_registration_does_not_rebuild_and_remove_is_explicit() {
        let mut services =
            DocumentLayoutServices::with_system_font_policy(SystemFontPolicy::Disabled);
        let registration = registration("rule-11", "Stable Alias", TEST_TTF);
        services.register_web_font(registration.clone()).unwrap();
        let font_context_address = std::ptr::from_ref(&services.parley_mut().font_context);

        assert_eq!(
            services.register_web_font(registration),
            Ok(WebFontRegistrationOutcome::Unchanged)
        );
        assert_eq!(
            font_context_address,
            std::ptr::from_ref(&services.parley_mut().font_context)
        );
        assert!(services.remove_web_font("rule-11"));
        assert!(!services.remove_web_font("rule-11"));
        assert!(!has_family(&mut services, "Stable Alias"));
    }

    #[test]
    fn descriptor_validation_rejects_non_css_metadata() {
        let mut services = DocumentLayoutServices::new();
        for face in [
            WebFontFace::new(""),
            WebFontFace::new("Bad Weight").with_weight(0.0),
            WebFontFace::new("Bad Stretch").with_stretch(f32::NAN),
            WebFontFace::new("Bad Style").with_style(WebFontStyle::Oblique(Some(f32::INFINITY))),
        ] {
            let error = services
                .register_web_font(WebFontRegistration::new("slot", face, TEST_TTF.to_vec()))
                .unwrap_err();
            assert!(matches!(
                error,
                WebFontRegistrationError::InvalidDescriptor { .. }
            ));
        }
        assert_eq!(services.web_font_count(), 0);
    }
}
