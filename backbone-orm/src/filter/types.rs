//! Filter types: operators, values, sorting

/// Filter operator types
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOperator {
    // Comparison
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,

    // Pattern matching
    Like,
    ILike,
    NotLike,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,

    // Set operations
    In,
    NotIn,
    Between,
    NotBetween,

    // Null checks
    IsNull,
    IsNotNull,

    // Logical
    Or,
}

impl FilterOperator {
    /// Parse operator from string
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            // Equality operators
            "eq" | "where" => Some(FilterOperator::Equal),
            "noteq" | "ne" => Some(FilterOperator::NotEqual),

            // Comparison operators
            "gt" => Some(FilterOperator::GreaterThan),
            "gte" | "gteq" => Some(FilterOperator::GreaterThanOrEqual),
            "lt" => Some(FilterOperator::LessThan),
            "lte" | "lteq" => Some(FilterOperator::LessThanOrEqual),

            // Pattern matching
            "like" => Some(FilterOperator::Like),
            "ilike" => Some(FilterOperator::ILike),
            "notlike" => Some(FilterOperator::NotLike),
            "contain" => Some(FilterOperator::Contains),
            "notcontain" => Some(FilterOperator::NotContains),
            "startwith" | "startswith" => Some(FilterOperator::StartsWith),
            "endwith" | "endswith" => Some(FilterOperator::EndsWith),

            // Set operations
            "in" => Some(FilterOperator::In),
            "notin" => Some(FilterOperator::NotIn),
            "between" => Some(FilterOperator::Between),
            "notbetween" => Some(FilterOperator::NotBetween),

            // Null checks
            "isnull" => Some(FilterOperator::IsNull),
            "isnotnull" => Some(FilterOperator::IsNotNull),

            // Logical
            "or" | "orwhere" => Some(FilterOperator::Or),

            _ => None,
        }
    }

    /// Get SQL operator string
    pub fn as_sql(&self) -> &str {
        match self {
            FilterOperator::Equal => "=",
            FilterOperator::NotEqual => "!=",
            FilterOperator::GreaterThan => ">",
            FilterOperator::GreaterThanOrEqual => ">=",
            FilterOperator::LessThan => "<",
            FilterOperator::LessThanOrEqual => "<=",
            FilterOperator::Like => "LIKE",
            FilterOperator::ILike => "ILIKE",
            FilterOperator::NotLike => "NOT LIKE",
            FilterOperator::Contains => "LIKE",  // Wrapped in %...%
            FilterOperator::NotContains => "NOT LIKE",
            FilterOperator::StartsWith => "LIKE",  // Wrapped in ...%
            FilterOperator::EndsWith => "LIKE",    // Wrapped in %...
            FilterOperator::In => "IN",
            FilterOperator::NotIn => "NOT IN",
            FilterOperator::Between => "BETWEEN",
            FilterOperator::NotBetween => "NOT BETWEEN",
            FilterOperator::IsNull => "IS NULL",
            FilterOperator::IsNotNull => "IS NOT NULL",
            FilterOperator::Or => "OR",
        }
    }
}

/// Filter condition value
#[derive(Debug, Clone)]
pub enum FilterValue {
    Single(String),
    Multiple(Vec<String>),
    Null,
}

impl FilterValue {
    /// Parse from string (handles comma-separated values)
    pub fn from_string(s: String, is_array: bool) -> Self {
        if is_array {
            FilterValue::Multiple(s.split(',').map(|s| s.trim().to_string()).collect())
        } else {
            FilterValue::Single(s)
        }
    }

    /// Get as single value
    pub fn as_single(&self) -> Option<&String> {
        match self {
            FilterValue::Single(v) => Some(v),
            _ => None,
        }
    }

    /// Get as multiple values
    pub fn as_multiple(&self) -> Option<&Vec<String>> {
        match self {
            FilterValue::Multiple(v) => Some(v),
            _ => None,
        }
    }

    /// Check if is null
    pub fn is_null(&self) -> bool {
        matches!(self, FilterValue::Null)
    }
}

/// Logical operator for combining conditions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterLogical {
    And,
    Or,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "desc" | "-1" => SortDirection::Desc,
            _ => SortDirection::Asc,
        }
    }
}

/// Sort specification
#[derive(Debug, Clone)]
pub struct SortSpec {
    pub field: String,
    pub direction: SortDirection,
}

impl SortSpec {
    pub fn new(field: String, direction: SortDirection) -> Self {
        Self { field, direction }
    }
}
