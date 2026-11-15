//! Renderer-neutral dashboard widget contracts shared by the host and plugins.

use std::{collections::BTreeMap, error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Stable namespaced identity for a dashboard widget.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DashboardWidgetId(Box<str>);

impl DashboardWidgetId {
    /// Creates a namespaced widget ID such as `tuilane.tasks.due`.
    pub fn new(value: impl AsRef<str>) -> Result<Self, DashboardWidgetIdError> {
        let value = value.as_ref();
        let valid = value.len() <= 128
            && value.split('.').count() >= 2
            && value.split('.').all(|segment| {
                !segment.is_empty()
                    && segment.len() <= 63
                    && segment.as_bytes()[0].is_ascii_lowercase()
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_".contains(&byte)
                    })
            });
        if !valid {
            return Err(DashboardWidgetIdError);
        }
        Ok(Self(value.into()))
    }

    /// Returns the stable namespaced ID.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DashboardWidgetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for DashboardWidgetId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DashboardWidgetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Error returned when a dashboard widget ID is malformed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DashboardWidgetIdError;

impl fmt::Display for DashboardWidgetIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "dashboard widget IDs must be namespaced lowercase segments using letters, digits, '-' or '_'",
        )
    }
}

impl Error for DashboardWidgetIdError {}

/// Preferred width for a widget in the host's constrained dashboard grid.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DashboardSize {
    /// Occupies one grid column.
    #[default]
    Compact,
    /// Occupies two grid columns when the terminal is wide enough.
    Wide,
    /// Occupies the full available row.
    Full,
}

/// A validated choice offered by a widget setting.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DashboardChoice(Box<str>);

impl DashboardChoice {
    /// Creates a choice value.
    pub fn new(value: impl Into<Box<str>>) -> Self {
        Self(value.into())
    }

    /// Returns the choice value.
    pub fn value(&self) -> &str {
        &self.0
    }
}

/// A setting declared by a dashboard widget.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DashboardOption {
    id: Box<str>,
    label: Box<str>,
    kind: DashboardOptionKind,
}

impl DashboardOption {
    /// Creates a setting with a host-editable value type and default.
    pub fn new(
        id: impl Into<Box<str>>,
        label: impl Into<Box<str>>,
        kind: DashboardOptionKind,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
        }
    }

    /// Returns the stable widget-local setting key.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the user-facing setting label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the setting's choices and default value.
    pub fn kind(&self) -> &DashboardOptionKind {
        &self.kind
    }

    /// Returns this option's declared default value.
    pub fn default_value(&self) -> DashboardOptionValue {
        self.kind.default_value()
    }

    /// Checks whether a saved value matches this option's declared bounds.
    pub fn accepts(&self, value: &DashboardOptionValue) -> bool {
        self.kind.accepts(value)
    }
}

/// Supported host-rendered dashboard setting types.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum DashboardOptionKind {
    /// Switches a widget behavior on or off.
    Toggle { default: bool },
    /// Sets a bounded integer value.
    Number { default: u8, min: u8, max: u8 },
    /// Selects one of a fixed set of values.
    Choice {
        default: Box<str>,
        choices: Vec<DashboardChoice>,
    },
}

impl DashboardOptionKind {
    /// Returns the option's declared default in the shared saved-value format.
    pub fn default_value(&self) -> DashboardOptionValue {
        match self {
            Self::Toggle { default } => DashboardOptionValue::Toggle(*default),
            Self::Number { default, min, max } => {
                DashboardOptionValue::Number((*default).clamp(*min, (*max).max(*min)))
            }
            Self::Choice { default, choices } => DashboardOptionValue::Choice(
                choices
                    .iter()
                    .find(|choice| choice.value() == default.as_ref())
                    .map_or_else(
                        || {
                            choices
                                .first()
                                .map_or_else(|| default.clone(), |item| item.value().into())
                        },
                        |choice| choice.value().into(),
                    ),
            ),
        }
    }

    /// Checks the type and declared range of a saved value.
    pub fn accepts(&self, value: &DashboardOptionValue) -> bool {
        match (self, value) {
            (Self::Toggle { .. }, DashboardOptionValue::Toggle(_)) => true,
            (Self::Number { min, max, .. }, DashboardOptionValue::Number(value)) => {
                value >= min && value <= max
            }
            (Self::Choice { choices, .. }, DashboardOptionValue::Choice(value)) => choices
                .iter()
                .any(|choice| choice.value() == value.as_ref()),
            _ => false,
        }
    }
}

/// A saved dashboard setting value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum DashboardOptionValue {
    /// Boolean value used by a toggle setting.
    Toggle(bool),
    /// Bounded integer value used by a numeric setting.
    Number(u8),
    /// Selected value used by a choice setting.
    Choice(Box<str>),
}

/// User-controlled visibility, size, and options for one dashboard widget.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardWidgetPreference {
    id: DashboardWidgetId,
    #[serde(default = "default_visible")]
    visible: bool,
    #[serde(default)]
    size: DashboardSize,
    #[serde(default)]
    options: BTreeMap<Box<str>, DashboardOptionValue>,
}

fn default_visible() -> bool {
    true
}

impl DashboardWidgetPreference {
    /// Creates visible widget preferences with no plugin-specific options.
    pub fn new(id: DashboardWidgetId, size: DashboardSize) -> Self {
        Self {
            id,
            visible: true,
            size,
            options: BTreeMap::new(),
        }
    }

    /// Returns the widget ID.
    pub fn id(&self) -> &DashboardWidgetId {
        &self.id
    }

    /// Returns whether the widget is shown on the dashboard.
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Sets whether the widget is shown on the dashboard.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Returns the preferred grid width.
    pub fn size(&self) -> DashboardSize {
        self.size
    }

    /// Sets the preferred grid width.
    pub fn set_size(&mut self, size: DashboardSize) {
        self.size = size;
    }

    /// Returns a saved value for one widget-local option.
    pub fn option(&self, id: &str) -> Option<&DashboardOptionValue> {
        self.options.get(id)
    }

    /// Stores a widget-local option value.
    pub fn set_option(&mut self, id: impl Into<Box<str>>, value: DashboardOptionValue) {
        self.options.insert(id.into(), value);
    }

    /// Returns all saved widget-local option values.
    pub fn options(&self) -> &BTreeMap<Box<str>, DashboardOptionValue> {
        &self.options
    }
}

/// Ordered set of saved dashboard widget preferences.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct DashboardLayout {
    widgets: Vec<DashboardWidgetPreference>,
}

impl<'de> Deserialize<'de> for DashboardLayout {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Default, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct Fields {
            widgets: Vec<DashboardWidgetPreference>,
        }

        let fields = Fields::deserialize(deserializer)?;
        let mut ids = std::collections::HashSet::with_capacity(fields.widgets.len());
        if fields.widgets.iter().any(|widget| !ids.insert(widget.id())) {
            return Err(de::Error::custom(
                "dashboard layout contains duplicate widget IDs",
            ));
        }
        Ok(Self {
            widgets: fields.widgets,
        })
    }
}

impl DashboardLayout {
    /// Creates a dashboard layout with the supplied widget order.
    pub fn new(widgets: Vec<DashboardWidgetPreference>) -> Self {
        let mut layout = Self::default();
        for widget in widgets {
            layout.set_preference(widget);
        }
        layout
    }

    /// Returns saved widgets in their dashboard order.
    pub fn widgets(&self) -> &[DashboardWidgetPreference] {
        &self.widgets
    }

    /// Finds saved preferences for a widget.
    pub fn preference(&self, id: &DashboardWidgetId) -> Option<&DashboardWidgetPreference> {
        self.widgets.iter().find(|widget| widget.id() == id)
    }

    /// Finds mutable saved preferences for a widget.
    pub fn preference_mut(
        &mut self,
        id: &DashboardWidgetId,
    ) -> Option<&mut DashboardWidgetPreference> {
        self.widgets.iter_mut().find(|widget| widget.id() == id)
    }

    /// Inserts or replaces one preference while preserving the current order.
    pub fn set_preference(&mut self, preference: DashboardWidgetPreference) {
        if let Some(existing) = self.preference_mut(preference.id()) {
            *existing = preference;
        } else {
            self.widgets.push(preference);
        }
    }

    /// Moves a widget by one position and returns whether its position changed.
    pub fn move_widget(&mut self, id: &DashboardWidgetId, offset: isize) -> bool {
        let Some(index) = self.widgets.iter().position(|widget| widget.id() == id) else {
            return false;
        };
        let target = index.saturating_add_signed(offset);
        if target >= self.widgets.len() || target == index {
            return false;
        }
        let widget = self.widgets.remove(index);
        self.widgets.insert(target, widget);
        true
    }
}

/// Static metadata for a host-rendered dashboard widget.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DashboardWidgetDescriptor {
    id: DashboardWidgetId,
    title: Box<str>,
    description: Box<str>,
    default_size: DashboardSize,
    options: Vec<DashboardOption>,
}

impl DashboardWidgetDescriptor {
    /// Creates metadata for a dashboard widget.
    pub fn new(
        id: DashboardWidgetId,
        title: impl Into<Box<str>>,
        description: impl Into<Box<str>>,
        default_size: DashboardSize,
        options: Vec<DashboardOption>,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            description: description.into(),
            default_size,
            options,
        }
    }

    /// Returns the stable widget ID.
    pub fn id(&self) -> &DashboardWidgetId {
        &self.id
    }

    /// Returns the widget title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the widget description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns the default grid width.
    pub fn default_size(&self) -> DashboardSize {
        self.default_size
    }

    /// Returns the settings declared by this widget.
    pub fn options(&self) -> &[DashboardOption] {
        &self.options
    }
}

/// One host-renderable data row in a dashboard widget.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DashboardItem {
    title: Box<str>,
    detail: Option<Box<str>>,
    tone: DashboardTone,
}

impl DashboardItem {
    /// Creates a plain dashboard row with optional supporting detail.
    pub fn new(title: impl Into<Box<str>>, detail: Option<Box<str>>) -> Self {
        Self {
            title: title.into(),
            detail,
            tone: DashboardTone::Default,
        }
    }

    /// Sets the semantic emphasis used by the host renderer.
    pub fn with_tone(mut self, tone: DashboardTone) -> Self {
        self.tone = tone;
        self
    }

    /// Returns the row title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns supporting detail.
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Returns the semantic emphasis.
    pub fn tone(&self) -> DashboardTone {
        self.tone
    }
}

/// Semantic emphasis interpreted by the host's active theme.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DashboardTone {
    /// Standard terminal foreground.
    #[default]
    Default,
    /// Secondary or less prominent content.
    Muted,
    /// Primary accent content.
    Accent,
    /// Successful or completed state.
    Positive,
    /// Due soon or attention-needed state.
    Warning,
    /// Overdue or failed state.
    Critical,
}

/// Structured data a plugin can provide without drawing terminal UI itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "type",
    content = "content",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum DashboardWidgetContent {
    /// One large value with a short caption and optional context.
    Metric {
        value: Box<str>,
        label: Box<str>,
        detail: Option<Box<str>>,
        /// Semantic emphasis interpreted by the host theme.
        #[serde(default)]
        tone: DashboardTone,
    },
    /// A bounded collection of rows.
    List {
        items: Vec<DashboardItem>,
        empty_message: Box<str>,
    },
    /// A short informational state, such as a provider error.
    Message {
        title: Box<str>,
        detail: Box<str>,
        tone: DashboardTone,
    },
}

/// Current data snapshot for one dashboard widget.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DashboardWidget {
    descriptor: DashboardWidgetDescriptor,
    content: DashboardWidgetContent,
}

impl DashboardWidget {
    /// Creates a dashboard widget from its descriptor and current data.
    pub fn new(descriptor: DashboardWidgetDescriptor, content: DashboardWidgetContent) -> Self {
        Self {
            descriptor,
            content,
        }
    }

    /// Returns stable widget metadata.
    pub fn descriptor(&self) -> &DashboardWidgetDescriptor {
        &self.descriptor
    }

    /// Returns the current widget data.
    pub fn content(&self) -> &DashboardWidgetContent {
        &self.content
    }
}

/// Tests for dashboard identifiers and persisted preferences.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_ids_require_namespaced_safe_segments() {
        for value in ["tuilane.tasks.due", "org.example.widget_1"] {
            assert_eq!(DashboardWidgetId::new(value).unwrap().as_str(), value);
        }
        for value in ["", "Tasks.due", "tasks", "tasks..due", "tasks.due/now"] {
            assert!(DashboardWidgetId::new(value).is_err());
        }
    }

    #[test]
    fn layout_preserves_widget_order_and_settings_in_toml() {
        let mut first = DashboardWidgetPreference::new(
            DashboardWidgetId::new("tuilane.tasks.due").unwrap(),
            DashboardSize::Wide,
        );
        first.set_option("limit", DashboardOptionValue::Number(8));
        let second = DashboardWidgetPreference::new(
            DashboardWidgetId::new("tuilane.notes.recent").unwrap(),
            DashboardSize::Compact,
        );
        let layout = DashboardLayout::new(vec![first, second]);

        let encoded = toml::to_string(&layout).unwrap();
        let decoded: DashboardLayout = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, layout);

        let mut reordered = decoded;
        assert!(
            reordered.move_widget(&DashboardWidgetId::new("tuilane.notes.recent").unwrap(), -1)
        );
        assert_eq!(reordered.widgets()[0].id().as_str(), "tuilane.notes.recent");
    }

    #[test]
    fn deserialization_rejects_invalid_widget_ids() {
        let decoded = toml::from_str::<DashboardLayout>(
            "[[widgets]]\nid = 'invalid'\nvisible = true\nsize = 'wide'\n",
        );
        assert!(decoded.is_err());
    }

    #[test]
    fn deserialization_rejects_duplicate_widget_ids() {
        let decoded = toml::from_str::<DashboardLayout>(
            "[[widgets]]\nid = 'tuilane.tasks.open'\n\n[[widgets]]\nid = 'tuilane.tasks.open'\n",
        );
        assert!(decoded.is_err());
    }

    #[test]
    fn widget_data_uses_a_renderer_neutral_json_contract() {
        let widget = DashboardWidget::new(
            DashboardWidgetDescriptor::new(
                DashboardWidgetId::new("org.example.status.ready").unwrap(),
                "Status",
                "Current service status",
                DashboardSize::Compact,
                Vec::new(),
            ),
            DashboardWidgetContent::Metric {
                value: "Ready".into(),
                label: "Service".into(),
                detail: Some("Listening locally".into()),
                tone: DashboardTone::Positive,
            },
        );
        let encoded = serde_json::to_string(&widget).unwrap();
        let decoded: DashboardWidget = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, widget);
    }
}
