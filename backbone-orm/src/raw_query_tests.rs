//! Unit tests for Raw Query module

#[cfg(test)]
mod tests {
    use super::super::raw_query::{
        RawQueryBuilder, AdvancedQueryBuilder,
        JoinType, CteClause, WindowFunction
    };
    use crate::query_builder::QueryValue;

    #[test]
    fn test_raw_query_builder_basic() {
        let query = RawQueryBuilder::new("SELECT * FROM users WHERE id = $1")
            .bind(QueryValue::integer(42))
            .build();

        assert!(query.0.contains("SELECT * FROM users WHERE id ="));
        assert_eq!(query.1.len(), 1);
        match &query.1[0] {
            QueryValue::Integer(id) => assert_eq!(*id, 42),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_raw_query_builder_multiple_params() {
        let query = RawQueryBuilder::new("SELECT * FROM users WHERE age > $1 AND status = $2")
            .bind(QueryValue::integer(18))
            .bind(QueryValue::text("active"))
            .build();

        assert_eq!(query.1.len(), 2);
        match &query.1[0] {
            QueryValue::Integer(age) => assert_eq!(*age, 18),
            _ => assert!(false),
        }
        match &query.1[1] {
            QueryValue::Text(status) => assert_eq!(status, "active"),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_raw_query_builder_bind_many() {
        let params = vec![
            QueryValue::text("user1"),
            QueryValue::text("user2"),
            QueryValue::text("user3"),
        ];

        let query = RawQueryBuilder::new("SELECT * FROM users WHERE name IN ($1, $2, $3)")
            .bind_many(params)
            .build();

        assert_eq!(query.1.len(), 3);

        for (i, param) in query.1.iter().enumerate() {
            match param {
                QueryValue::Text(name) => {
                    let expected = format!("user{}", i + 1);
                    assert_eq!(name, &expected);
                },
                _ => assert!(false),
            }
        }
    }

    #[test]
    fn test_raw_query_builder_all_value_types() {
        let uuid_val = uuid::Uuid::new_v4();
        let timestamp_val = chrono::Utc::now().naive_utc();

        let query = RawQueryBuilder::new("INSERT INTO test VALUES ($1, $2, $3, $4, $5, $6, $7)")
            .bind(QueryValue::text("text"))
            .bind(QueryValue::integer(42))
            .bind(QueryValue::float(3.14))
            .bind(QueryValue::boolean(true))
            .bind(QueryValue::Uuid(uuid_val))
            .bind(QueryValue::Timestamp(timestamp_val))
            .bind(QueryValue::null())
            .build();

        assert_eq!(query.1.len(), 7);

        match &query.1[0] {
            QueryValue::Text(_) => assert!(true),
            _ => assert!(false),
        }
        match &query.1[1] {
            QueryValue::Integer(_) => assert!(true),
            _ => assert!(false),
        }
        match &query.1[2] {
            QueryValue::Float(_) => assert!(true),
            _ => assert!(false),
        }
        match &query.1[3] {
            QueryValue::Boolean(_) => assert!(true),
            _ => assert!(false),
        }
        match &query.1[4] {
            QueryValue::Uuid(_) => assert!(true),
            _ => assert!(false),
        }
        match &query.1[5] {
            QueryValue::Timestamp(_) => assert!(true),
            _ => assert!(false),
        }
        match &query.1[6] {
            QueryValue::Null => assert!(true),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_advanced_query_builder_basic() {
        let query = AdvancedQueryBuilder::new("users")
            .select(&["id", "name", "email"])
            .where_raw("age > $1", QueryValue::integer(18))
            .order_by("name", "ASC")
            .limit(10)
            .build_sql();

        let expected_parts = vec![
            "SELECT id, name, email",
            "FROM users",
            "WHERE age > $1",
            "ORDER BY name ASC",
            "LIMIT 10"
        ];

        for part in expected_parts {
            assert!(query.0.contains(part));
        }
        assert_eq!(query.1.len(), 1);
    }

    #[test]
    fn test_advanced_query_builder_joins() {
        let query = AdvancedQueryBuilder::new("users")
            .select(&["users.name", "orders.total"])
            .join(JoinType::Inner, "orders", "users.id = orders.user_id")
            .join_alias(JoinType::Left, "profiles", "p", "users.id = p.user_id")
            .where_raw("orders.status = $1", QueryValue::text("completed"))
            .build_sql();

        assert!(query.0.contains("SELECT users.name, orders.total"));
        assert!(query.0.contains("FROM users"));
        assert!(query.0.contains("INNER JOIN orders ON users.id = orders.user_id"));
        assert!(query.0.contains("LEFT JOIN profiles AS p ON users.id = p.user_id"));
        assert!(query.0.contains("WHERE orders.status = $1"));
    }

    #[test]
    fn test_advanced_query_builder_all_join_types() {
        let query = AdvancedQueryBuilder::new("main")
            .join(JoinType::Inner, "table1", "main.id = table1.main_id")
            .join(JoinType::Left, "table2", "main.id = table2.main_id")
            .join(JoinType::Right, "table3", "main.id = table3.main_id")
            .join(JoinType::Full, "table4", "main.id = table4.main_id")
            .join(JoinType::Cross, "table5", "TRUE")
            .build_sql();

        assert!(query.0.contains("INNER JOIN table1"));
        assert!(query.0.contains("LEFT JOIN table2"));
        assert!(query.0.contains("RIGHT JOIN table3"));
        assert!(query.0.contains("FULL JOIN table4"));
        assert!(query.0.contains("CROSS JOIN table5"));
    }

    #[test]
    fn test_advanced_query_builder_group_by_having() {
        let query = AdvancedQueryBuilder::new("orders")
            .select(&["user_id", "COUNT(*) as order_count", "SUM(total) as total_spent"])
            .group_by(&["user_id"])
            .having("COUNT(*) > $1", QueryValue::integer(5))
            .having("SUM(total) > $2", QueryValue::float(1000.0))
            .order_by("total_spent", "DESC")
            .build_sql();

        assert!(query.0.contains("SELECT user_id, COUNT(*) as order_count, SUM(total) as total_spent"));
        assert!(query.0.contains("GROUP BY user_id"));
        assert!(query.0.contains("HAVING COUNT(*) > $1 AND SUM(total) > $2"));
        assert!(query.0.contains("ORDER BY total_spent DESC"));
        assert_eq!(query.1.len(), 2); // Two HAVING parameters
    }

    #[test]
    fn test_advanced_query_builder_cte() {
        let query = AdvancedQueryBuilder::new("users")
            .with_cte("user_stats", "SELECT user_id, COUNT(*) as order_count FROM orders GROUP BY user_id")
            .with_cte("active_users", "SELECT id FROM users WHERE last_login > NOW() - INTERVAL '30 days'")
            .select(&["users.name", "user_stats.order_count"])
            .join(JoinType::Inner, "user_stats", "users.id = user_stats.user_id")
            .join(JoinType::Inner, "active_users", "users.id = active_users.id")
            .build_sql();

        assert!(query.0.starts_with("WITH"));
        assert!(query.0.contains("user_stats AS (SELECT user_id, COUNT(*) as order_count FROM orders GROUP BY user_id)"));
        assert!(query.0.contains("active_users AS (SELECT id FROM users WHERE last_login > NOW() - INTERVAL '30 days')"));
        assert!(query.0.contains("SELECT users.name, user_stats.order_count"));
    }

    #[test]
    fn test_advanced_query_builder_window_functions() {
        let query = AdvancedQueryBuilder::new("employees")
            .select(&["name", "salary", "department"])
            .window_fn("ROW_NUMBER()", "row_num", &["department"], &["salary DESC"])
            .window_fn("AVG(salary)", "dept_avg", &["department"], &[])
            .window_fn_with_frame(
                "SUM(salary) OVER (PARTITION BY department ORDER BY hire_date ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)",
                "running_total",
                &["department"],
                &["hire_date"],
                "ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW"
            )
            .build_sql();

        assert!(query.0.contains("ROW_NUMBER() OVER (PARTITION BY department ORDER BY salary DESC) AS row_num"));
        assert!(query.0.contains("AVG(salary) OVER (PARTITION BY department) AS dept_avg"));
        assert!(query.0.contains("SUM(salary) OVER (PARTITION BY department ORDER BY hire_date ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS running_total"));
    }

    #[test]
    fn test_advanced_query_builder_complex_query() {
        let query = AdvancedQueryBuilder::new("sales")
            .with_cte("regional_totals", "SELECT region, SUM(amount) as total FROM sales GROUP BY region")
            .select(&[
                "sales.product",
                "sales.amount",
                "regional_totals.total",
                "ROW_NUMBER() OVER (PARTITION BY sales.region ORDER BY sales.amount DESC) as rank_in_region"
            ])
            .join(JoinType::Inner, "regional_totals", "sales.region = regional_totals.region")
            .where_raw("sales.date >= $1", QueryValue::text("2024-01-01"))
            .where_raw("sales.amount > $2", QueryValue::float(1000.0))
            .group_by(&["sales.product", "sales.region", "regional_totals.total"])
            .having("COUNT(*) > $3", QueryValue::integer(10))
            .order_by("rank_in_region", "ASC")
            .limit(100)
            .offset(50)
            .build_sql();

        // Verify CTE
        assert!(query.0.contains("WITH regional_totals AS (SELECT region, SUM(amount) as total FROM sales GROUP BY region)"));

        // Verify SELECT with window function
        assert!(query.0.contains("ROW_NUMBER() OVER (PARTITION BY sales.region ORDER BY sales.amount DESC) as rank_in_region"));

        // Verify JOIN
        assert!(query.0.contains("INNER JOIN regional_totals ON sales.region = regional_totals.region"));

        // Verify WHERE conditions
        assert!(query.0.contains("sales.date >= $1 AND sales.amount > $2"));

        // Verify GROUP BY and HAVING
        assert!(query.0.contains("GROUP BY sales.product, sales.region, regional_totals.total"));
        assert!(query.0.contains("HAVING COUNT(*) > $3"));

        // Verify ORDER BY, LIMIT, OFFSET
        assert!(query.0.contains("ORDER BY rank_in_region ASC LIMIT 100 OFFSET 50"));

        // Verify parameter count
        assert_eq!(query.1.len(), 3);
    }

    #[test]
    fn test_join_clause_struct() {
        use super::super::raw_query::JoinClause;

        let join = JoinClause {
            join_type: JoinType::Left,
            table: "profiles".to_string(),
            on_condition: "users.id = profiles.user_id".to_string(),
            alias: Some("p".to_string()),
        };

        match join.join_type {
            JoinType::Left => assert!(true),
            _ => assert!(false),
        }
        assert_eq!(join.table, "profiles");
        assert_eq!(join.on_condition, "users.id = profiles.user_id");
        assert_eq!(join.alias, Some("p".to_string()));
    }

    #[test]
    fn test_cte_clause_struct() {
        let cte = CteClause {
            name: "user_summary".to_string(),
            query: "SELECT user_id, COUNT(*) as order_count FROM orders GROUP BY user_id".to_string(),
        };

        assert_eq!(cte.name, "user_summary");
        assert!(cte.query.contains("COUNT(*)"));
        assert!(cte.query.contains("GROUP BY user_id"));
    }

    #[test]
    fn test_window_function_struct() {
        let wf = WindowFunction {
            expression: "ROW_NUMBER()".to_string(),
            alias: "row_num".to_string(),
            partition_by: vec!["department".to_string()],
            order_by: vec!["salary DESC".to_string()],
            frame: Some("ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW".to_string()),
        };

        assert_eq!(wf.expression, "ROW_NUMBER()");
        assert_eq!(wf.alias, "row_num");
        assert_eq!(wf.partition_by[0], "department");
        assert_eq!(wf.order_by[0], "salary DESC");
        assert!(wf.frame.is_some());
    }

    #[test]
    fn test_advanced_query_builder_empty() {
        let query = AdvancedQueryBuilder::new("test_table").build_sql();

        assert!(query.0.contains("SELECT * FROM test_table"));
        assert_eq!(query.1.len(), 0);
    }

    #[test]
    fn test_advanced_query_builder_only_cte() {
        let query = AdvancedQueryBuilder::new("main")
            .with_cte("temp_data", "SELECT * FROM source_table WHERE active = true")
            .build_sql();

        assert!(query.0.starts_with("WITH temp_data AS (SELECT * FROM source_table WHERE active = true)"));
        assert!(query.0.contains("SELECT * FROM main"));
    }

    #[test]
    fn test_advanced_query_builder_only_window_functions() {
        let query = AdvancedQueryBuilder::new("sales")
            .window_fn("SUM(amount)", "running_total", &["region"], &["date"])
            .window_fn("COUNT(*)", "count_over_time", &[], &["date"])
            .build_sql();

        assert!(query.0.contains("SUM(amount) OVER (PARTITION BY region ORDER BY date) AS running_total"));
        assert!(query.0.contains("COUNT(*) OVER (ORDER BY date) AS count_over_time"));
    }
}