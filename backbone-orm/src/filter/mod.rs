//! Query filter system inspired by Laravel Filter Query String
//!
//! Provides a flexible, type-safe filtering system for database queries.
//! Supports complex query expressions through URL-friendly parameters.
//!
//! # Example Query String
//!
//! ```text
//! ?username[contain]=john&age[gt]=18&status[eq]=active&orderby=name&limit=10
//! ```
//!
//! # Supported Operators
//!
//! | Operator | Description | SQL Equivalent |
//! |----------|-------------|----------------|
//! | `eq` | Equal | `field = value` |
//! | `notEq` | Not equal | `field != value` |
//! | `gt` | Greater than | `field > value` |
//! | `gte` / `gtEq` | Greater or equal | `field >= value` |
//! | `lt` | Less than | `field < value` |
//! | `lte` / `ltEq` | Less or equal | `field <= value` |
//! | `like` | Case-sensitive LIKE | `field LIKE value` |
//! | `ilike` | Case-insensitive LIKE | `field ILIKE value` |
//! | `notlike` | NOT LIKE | `field NOT LIKE value` |
//! | `contain` | Contains | `field LIKE %value%` |
//! | `notcontain` | Does not contain | `field NOT LIKE %value%` |
//! | `startwith` | Starts with | `field LIKE value%` |
//! | `endwith` | Ends with | `field LIKE %value` |
//! | `in` | In array | `field IN (...)` |
//! | `notin` | Not in array | `field NOT IN (...)` |
//! | `between` | Between two values | `field BETWEEN a AND b` |
//! | `notbetween` | Not between | `field NOT BETWEEN a AND b` |
//! | `isnull` | Is null | `field IS NULL` |
//! | `isnotnull` | Is not null | `field IS NOT NULL` |
//! | `or` | OR condition | `OR field = value` |

mod types;
mod validation;
mod condition;
mod query;
mod parser;

pub use types::{FilterOperator, FilterValue, FilterLogical, SortDirection, SortSpec};
pub use validation::{FilterableEntity, is_valid_field, sanitize_field_name};
pub use condition::FilterCondition;
pub use query::QueryFilter;
pub use parser::parse_filters;

#[cfg(test)]
mod tests;
