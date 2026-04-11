//! Unit tests for QueryBuilder

#[cfg(test)]
mod tests {
    use super::super::query_builder::{QueryBuilder, QueryValue};
    use uuid::Uuid;
    use chrono::NaiveDateTime;

    #[test]
    fn test_query_builder_new() {
        let qb = QueryBuilder::new("users");
        assert_eq!(qb.build_sql(), "SELECT * FROM users");
    }

    #[test]
    fn test_select_fields() {
        let qb = QueryBuilder::new("users")
            .select(&["id", "name", "email"]);
        assert_eq!(qb.build_sql(), "SELECT id, name, email FROM users");
    }

    #[test]
    fn test_where_eq_single() {
        let qb = QueryBuilder::new("users")
            .where_eq("id", QueryValue::text("123"));
        assert_eq!(qb.build_sql(), "SELECT * FROM users WHERE id = $1");
    }

    #[test]
    fn test_where_eq_multiple() {
        let qb = QueryBuilder::new("users")
            .where_eq("name", QueryValue::text("John"))
            .where_eq("age", QueryValue::integer(25));
        assert_eq!(
            qb.build_sql(),
            "SELECT * FROM users WHERE name = $1 AND age = $2"
        );
    }

    #[test]
    fn test_where_conditions() {
        let qb = QueryBuilder::new("users")
            .where_eq("status", QueryValue::text("active"))
            .where_ne("deleted", QueryValue::boolean(true))
            .where_gt("age", QueryValue::integer(18))
            .where_lt("age", QueryValue::integer(65))
            .where_like("name", QueryValue::text("%John%"));

        assert_eq!(
            qb.build_sql(),
            "SELECT * FROM users WHERE status = $1 AND deleted != $2 AND age > $3 AND age < $4 AND name LIKE $5"
        );
    }

    #[test]
    fn test_where_in_single() {
        let values = vec![
            QueryValue::text("admin"),
            QueryValue::text("moderator"),
            QueryValue::text("user")
        ];
        let qb = QueryBuilder::new("users")
            .where_in("role", values);
        assert_eq!(qb.build_sql(), "SELECT * FROM users WHERE role IN ($1, $2, $3)");
    }

    #[test]
    fn test_where_in_empty() {
        let qb = QueryBuilder::new("users")
            .where_in("role", vec![]);
        assert_eq!(qb.build_sql(), "SELECT * FROM users");
    }

    #[test]
    fn test_order_by() {
        let qb = QueryBuilder::new("users")
            .order_by("name", "ASC");
        assert_eq!(qb.build_sql(), "SELECT * FROM users ORDER BY name ASC");
    }

    #[test]
    fn test_order_by_desc() {
        let qb = QueryBuilder::new("users")
            .order_by("created_at", "DESC");
        assert_eq!(qb.build_sql(), "SELECT * FROM users ORDER BY created_at DESC");
    }

    #[test]
    fn test_order_by_invalid_direction_defaults_to_asc() {
        let qb = QueryBuilder::new("users")
            .order_by("name", "INVALID");
        assert_eq!(qb.build_sql(), "SELECT * FROM users ORDER BY name ASC");
    }

    #[test]
    fn test_multiple_order_by() {
        let qb = QueryBuilder::new("users")
            .order_by("status", "DESC")
            .order_by("name", "ASC");
        assert_eq!(qb.build_sql(), "SELECT * FROM users ORDER BY status DESC, name ASC");
    }

    #[test]
    fn test_limit() {
        let qb = QueryBuilder::new("users")
            .limit(10);
        assert_eq!(qb.build_sql(), "SELECT * FROM users LIMIT 10");
    }

    #[test]
    fn test_offset() {
        let qb = QueryBuilder::new("users")
            .offset(20);
        assert_eq!(qb.build_sql(), "SELECT * FROM users OFFSET 20");
    }

    #[test]
    fn test_limit_and_offset() {
        let qb = QueryBuilder::new("users")
            .limit(10)
            .offset(20);
        assert_eq!(qb.build_sql(), "SELECT * FROM users LIMIT 10 OFFSET 20");
    }

    #[test]
    fn test_complex_query() {
        let qb = QueryBuilder::new("users")
            .select(&["id", "name", "email"])
            .where_eq("status", QueryValue::text("active"))
            .where_in("role", vec![
                QueryValue::text("admin"),
                QueryValue::text("moderator")
            ])
            .order_by("created_at", "DESC")
            .limit(10)
            .offset(20);

        assert_eq!(
            qb.build_sql(),
            "SELECT id, name, email FROM users WHERE status = $1 AND role IN ($2, $3) ORDER BY created_at DESC LIMIT 10 OFFSET 20"
        );
    }

    #[test]
    fn test_build_query_with_parameters() {
        let qb = QueryBuilder::new("users")
            .where_eq("name", QueryValue::text("John"))
            .where_gt("age", QueryValue::integer(25));

        let (sql, params) = qb.build_query();
        assert_eq!(sql, "SELECT * FROM users WHERE name = $1 AND age > $2");
        assert_eq!(params.len(), 2);

        match &params[0] {
            QueryValue::Text(value) => assert_eq!(value, "John"),
            _ => panic!("Expected Text value"),
        }

        match &params[1] {
            QueryValue::Integer(value) => assert_eq!(*value, 25),
            _ => panic!("Expected Integer value"),
        }
    }

    #[test]
    fn test_query_value_constructors() {
        // Test all QueryValue constructors
        let text_val = QueryValue::text("hello");
        let int_val = QueryValue::integer(42);
        let float_val = QueryValue::float(3.14);
        let bool_val = QueryValue::boolean(true);
        let uuid_val = QueryValue::uuid(Uuid::new_v4());
        let timestamp_val = QueryValue::timestamp(
            NaiveDateTime::from_timestamp_opt(1609459200, 0).unwrap()
        );
        let null_val = QueryValue::null();

        // Verify they can be created without panicking
        match text_val {
            QueryValue::Text(_) => assert!(true),
            _ => assert!(false),
        }

        match int_val {
            QueryValue::Integer(_) => assert!(true),
            _ => assert!(false),
        }

        match float_val {
            QueryValue::Float(_) => assert!(true),
            _ => assert!(false),
        }

        match bool_val {
            QueryValue::Boolean(_) => assert!(true),
            _ => assert!(false),
        }

        match uuid_val {
            QueryValue::Uuid(_) => assert!(true),
            _ => assert!(false),
        }

        match timestamp_val {
            QueryValue::Timestamp(_) => assert!(true),
            _ => assert!(false),
        }

        match null_val {
            QueryValue::Null => assert!(true),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_query_value_text_conversion() {
        let val = QueryValue::text("test");
        if let QueryValue::Text(s) = val {
            assert_eq!(s, "test");
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn test_query_value_integer_conversion() {
        let val = QueryValue::integer(123);
        if let QueryValue::Integer(i) = val {
            assert_eq!(i, 123);
        } else {
            panic!("Expected Integer variant");
        }
    }

    #[test]
    fn test_query_value_float_conversion() {
        let val = QueryValue::float(45.67);
        if let QueryValue::Float(f) = val {
            assert_eq!(f, 45.67);
        } else {
            panic!("Expected Float variant");
        }
    }

    #[test]
    fn test_query_value_boolean_conversion() {
        let val = QueryValue::boolean(false);
        if let QueryValue::Boolean(b) = val {
            assert_eq!(b, false);
        } else {
            panic!("Expected Boolean variant");
        }
    }

    #[test]
    fn test_chaining_methods() {
        let qb = QueryBuilder::new("posts")
            .select(&["id", "title", "content"])
            .where_eq("published", QueryValue::boolean(true))
            .where_gt("created_at", QueryValue::text("2023-01-01"))
            .order_by("created_at", "DESC")
            .limit(5);

        let sql = qb.build_sql();
        assert!(sql.contains("SELECT id, title, content FROM posts"));
        assert!(sql.contains("published = $1"));
        assert!(sql.contains("created_at > $2"));
        assert!(sql.contains("ORDER BY created_at DESC"));
        assert!(sql.contains("LIMIT 5"));
    }

    #[test]
    fn test_empty_where_conditions() {
        let qb = QueryBuilder::new("users");
        assert!(!qb.build_sql().contains("WHERE"));
    }

    #[test]
    fn test_empty_order_by() {
        let qb = QueryBuilder::new("users");
        assert!(!qb.build_sql().contains("ORDER BY"));
    }

    #[test]
    fn test_parameter_increment() {
        let qb = QueryBuilder::new("users")
            .where_eq("a", QueryValue::text("1"))
            .where_eq("b", QueryValue::text("2"))
            .where_eq("c", QueryValue::text("3"));

        let sql = qb.build_sql();
        assert!(sql.contains("$1"));
        assert!(sql.contains("$2"));
        assert!(sql.contains("$3"));
        assert!(!sql.contains("$4")); // Should not exist
    }
}