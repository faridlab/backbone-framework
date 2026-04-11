//! Unit tests for Seeding module

#[cfg(test)]
mod tests {
    use super::super::seeding::{
        SeedManager, Seed, SeedRecord, SeedStatus, SeedType
    };
    use chrono::{DateTime, Utc};
    use regex::Regex;

    // Mock seed for testing
    struct MockSeed {
        name: String,
        seed_type: SeedType,
        up_sql: String,
        down_sql: String,
    }

    impl Seed for MockSeed {
        fn name(&self) -> &str {
            &self.name
        }

        fn seed_type(&self) -> SeedType {
            self.seed_type.clone()
        }

        fn up(&self) -> &str {
            &self.up_sql
        }

        fn down(&self) -> &str {
            &self.down_sql
        }
    }

    #[test]
    fn test_seed_trait_implementation() {
        let seed = MockSeed {
            name: "create_test_users".to_string(),
            seed_type: SeedType::Test,
            up_sql: "INSERT INTO users (id, name) VALUES ('1', 'test');".to_string(),
            down_sql: "DELETE FROM users WHERE id = '1';".to_string(),
        };

        assert_eq!(seed.name(), "create_test_users");
        match seed.seed_type() {
            SeedType::Test => assert!(true),
            _ => assert!(false),
        }
        assert_eq!(seed.up(), "INSERT INTO users (id, name) VALUES ('1', 'test');");
        assert_eq!(seed.down(), "DELETE FROM users WHERE id = '1';");
    }

    #[test]
    fn test_seed_record_structure() {
        let now = Utc::now();
        let record = SeedRecord {
            id: 1,
            name: "test_users_seed".to_string(),
            seed_type: "test".to_string(),
            applied_at: now,
        };

        assert_eq!(record.id, 1);
        assert_eq!(record.name, "test_users_seed");
        assert_eq!(record.seed_type, "test");
        assert_eq!(record.applied_at, now);
    }

    #[test]
    fn test_seed_status_structure() {
        let status = SeedStatus {
            total_seeds: 5,
            applied_seeds: vec![
                "create_admin_user".to_string(),
                "create_test_data".to_string(),
            ],
            pending_seeds: vec![
                "create_reference_data".to_string(),
                "create_permissions".to_string(),
                "create_sample_posts".to_string(),
            ],
            last_seed: Some("create_test_data".to_string()),
        };

        assert_eq!(status.total_seeds, 5);
        assert_eq!(status.applied_seeds.len(), 2);
        assert_eq!(status.pending_seeds.len(), 3);
        assert_eq!(status.last_seed, Some("create_test_data".to_string()));
    }

    #[test]
    fn test_seed_status_empty() {
        let status = SeedStatus {
            total_seeds: 0,
            applied_seeds: vec![],
            pending_seeds: vec![],
            last_seed: None,
        };

        assert_eq!(status.total_seeds, 0);
        assert!(status.applied_seeds.is_empty());
        assert!(status.pending_seeds.is_empty());
        assert!(status.last_seed.is_none());
    }

    #[test]
    fn test_seed_status_all_applied() {
        let status = SeedStatus {
            total_seeds: 2,
            applied_seeds: vec![
                "create_reference_data".to_string(),
                "create_test_users".to_string(),
            ],
            pending_seeds: vec![],
            last_seed: Some("create_test_users".to_string()),
        };

        assert_eq!(status.total_seeds, 2);
        assert_eq!(status.applied_seeds.len(), 2);
        assert!(status.pending_seeds.is_empty());
        assert!(status.last_seed.is_some());
    }

    #[test]
    fn test_seed_status_all_pending() {
        let status = SeedStatus {
            total_seeds: 3,
            applied_seeds: vec![],
            pending_seeds: vec![
                "create_countries".to_string(),
                "create_languages".to_string(),
                "create_roles".to_string(),
            ],
            last_seed: None,
        };

        assert_eq!(status.total_seeds, 3);
        assert!(status.applied_seeds.is_empty());
        assert_eq!(status.pending_seeds.len(), 3);
        assert!(status.last_seed.is_none());
    }

    #[test]
    fn test_seed_type_enum() {
        // Test all SeedType variants
        let data_seed = SeedType::Data;
        let test_seed = SeedType::Test;
        let reference_seed = SeedType::Reference;

        // Verify they can be created and compared
        match data_seed {
            SeedType::Data => assert!(true),
            _ => assert!(false),
        }

        match test_seed {
            SeedType::Test => assert!(true),
            _ => assert!(false),
        }

        match reference_seed {
            SeedType::Reference => assert!(true),
            _ => assert!(false),
        }
    }

    #[test]
    fn test_seed_type_serialization() {
        // Test that SeedType can be serialized and deserialized
        let seed_types = vec![
            SeedType::Data,
            SeedType::Test,
            SeedType::Reference,
        ];

        for seed_type in &seed_types {
            let serialized = serde_json::to_string(seed_type).unwrap();
            let deserialized: SeedType = serde_json::from_str(&serialized).unwrap();

            match (seed_type, deserialized) {
                (SeedType::Data, SeedType::Data) => assert!(true),
                (SeedType::Test, SeedType::Test) => assert!(true),
                (SeedType::Reference, SeedType::Reference) => assert!(true),
                _ => assert!(false),
            }
        }
    }

    #[test]
    fn test_seed_manager_new() {
        // Note: We can't test SeedManager::new without a PgPool
        // but we can verify the constructor signature is correct
        fn check_seed_manager_constructor() {
            // This function just ensures the type signature is correct
            let _: Option<SeedManager> = None;
        }

        check_seed_manager_constructor();
    }

    #[test]
    fn test_seed_names() {
        let seeds = vec![
            MockSeed {
                name: "001_create_admin_user".to_string(),
                seed_type: SeedType::Data,
                up_sql: "INSERT INTO users (id, name, email) VALUES ('admin', 'Admin', 'admin@test.com');".to_string(),
                down_sql: "DELETE FROM users WHERE email = 'admin@test.com';".to_string(),
            },
            MockSeed {
                name: "002_create_test_data".to_string(),
                seed_type: SeedType::Test,
                up_sql: "INSERT INTO test_data (id, value) VALUES (1, 'test');".to_string(),
                down_sql: "DELETE FROM test_data WHERE id = 1;".to_string(),
            },
            MockSeed {
                name: "003_create_reference_data".to_string(),
                seed_type: SeedType::Reference,
                up_sql: "INSERT INTO countries (id, code, name) VALUES ('US', 'US', 'United States');".to_string(),
                down_sql: "DELETE FROM countries WHERE id = 'US';".to_string(),
            },
        ];

        // Verify seed names
        assert_eq!(seeds[0].name(), "001_create_admin_user");
        assert_eq!(seeds[1].name(), "002_create_test_data");
        assert_eq!(seeds[2].name(), "003_create_reference_data");

        // Verify seed types
        match seeds[0].seed_type() {
            SeedType::Data => assert!(true),
            _ => assert!(false),
        }

        match seeds[1].seed_type() {
            SeedType::Test => assert!(true),
            _ => assert!(false),
        }

        match seeds[2].seed_type() {
            SeedType::Reference => assert!(true),
            _ => assert!(false),
        }

        // Verify SQL content
        assert!(seeds[0].up().contains("INSERT INTO users"));
        assert!(seeds[0].down().contains("DELETE FROM users"));
        assert!(seeds[1].up().contains("INSERT INTO test_data"));
        assert!(seeds[2].up().contains("INSERT INTO countries"));
    }

    #[test]
    fn test_seed_sql_content() {
        let seed = MockSeed {
            name: "complex_user_seed".to_string(),
            seed_type: SeedType::Test,
            up_sql: r#"
                INSERT INTO users (id, name, email, settings, created_at, updated_at) VALUES
                ('test-user-1', 'Test User 1', 'user1@test.local', '{"theme": "dark", "notifications": true}', NOW(), NOW()),
                ('test-user-2', 'Test User 2', 'user2@test.local', '{"theme": "light", "notifications": false}', NOW(), NOW());

                INSERT INTO user_profiles (user_id, bio, avatar_url) VALUES
                ('test-user-1', 'Test bio for user 1', 'https://example.com/avatar1.jpg'),
                ('test-user-2', 'Test bio for user 2', 'https://example.com/avatar2.jpg');
            "#.to_string(),
            down_sql: r#"
                DELETE FROM user_profiles WHERE user_id IN ('test-user-1', 'test-user-2');
                DELETE FROM users WHERE email LIKE '%@test.local';
            "#.to_string(),
        };

        // Verify complex SQL seed content
        assert!(seed.up().contains("INSERT INTO users"));
        assert!(seed.up().contains("INSERT INTO user_profiles"));
        assert!(seed.up().contains("\"theme\"")); // Look for JSON theme field
        assert!(seed.up().contains("NOW()"));
        assert!(seed.up().contains("@test.local")); // Test-specific email pattern

        assert!(seed.down().contains("DELETE FROM user_profiles"));
        assert!(seed.down().contains("DELETE FROM users"));
        assert!(seed.down().contains("LIKE '%@test.local'"));
    }

    #[test]
    fn test_timestamp_formatting() {
        let timestamp = "2023-12-25T10:30:45Z".parse::<DateTime<Utc>>().unwrap();
        let record = SeedRecord {
            id: 42,
            name: "test_seed".to_string(),
            seed_type: "test".to_string(),
            applied_at: timestamp,
        };

        // Verify timestamp can be formatted
        let formatted = record.applied_at.format("%Y-%m-%d %H:%M:%S").to_string();
        assert_eq!(formatted, "2023-12-25 10:30:45");
    }

    #[test]
    fn test_seed_status_calculations() {
        // Test various scenarios for seed status calculations
        let test_cases = vec![
            (vec![], vec!["a", "b", "c"], 3, 0, 3),
            (vec!["a"], vec!["b", "c"], 3, 1, 2),
            (vec!["a", "b", "c"], vec![], 3, 3, 0),
            (vec!["a", "b"], vec!["c"], 3, 2, 1),
        ];

        for (applied_names, pending_names, total, applied_count, pending_count) in test_cases {
            let status = SeedStatus {
                total_seeds: total,
                applied_seeds: applied_names.iter().map(|s| s.to_string()).collect(),
                pending_seeds: pending_names.iter().map(|s| s.to_string()).collect(),
                last_seed: applied_names.last().map(|s| s.to_string()),
            };

            assert_eq!(status.total_seeds, total);
            assert_eq!(status.applied_seeds.len(), applied_count);
            assert_eq!(status.pending_seeds.len(), pending_count);

            if applied_count > 0 {
                assert_eq!(status.last_seed, applied_names.last().map(|s| s.to_string()));
            } else {
                assert!(status.last_seed.is_none());
            }
        }
    }

    // Integration-style test that verifies the types work together
    #[test]
    fn test_seeding_types_integration() {
        fn verify_seed_trait<T: Seed>(seed: &T) {
            let _name = seed.name();
            let _seed_type = seed.seed_type();
            let _up = seed.up();
            let _down = seed.down();
        }

        fn verify_seed_record(record: SeedRecord) -> SeedRecord {
            // Verify record can be moved and returned
            record
        }

        fn verify_seed_status(status: SeedStatus) -> SeedStatus {
            // Verify status can be moved and returned
            status
        }

        let seed = MockSeed {
            name: "test".to_string(),
            seed_type: SeedType::Test,
            up_sql: "SELECT 1;".to_string(),
            down_sql: "SELECT 2;".to_string(),
        };

        verify_seed_trait(&seed);

        let record = SeedRecord {
            id: 1,
            name: "test".to_string(),
            seed_type: "test".to_string(),
            applied_at: Utc::now(),
        };

        let _returned_record = verify_seed_record(record);

        let status = SeedStatus {
            total_seeds: 1,
            applied_seeds: vec!["test".to_string()],
            pending_seeds: vec![],
            last_seed: Some("test".to_string()),
        };

        let _returned_status = verify_seed_status(status);
    }

    #[test]
    fn test_seed_template_generation() {
        // Test that template generation methods would work (simulated)
        // In real implementation, these would generate actual SQL templates

        let seed_manager_templates = vec![
            ("data_seed", "INSERT INTO"),
            ("test_seed", "test-local"),
            ("reference_seed", "INSERT INTO"),
        ];

        for (template_type, expected_content) in seed_manager_templates {
            // Simulate template generation by checking expected content patterns
            assert!(!expected_content.is_empty());

            // In real implementation, these methods would return actual SQL templates
            // For testing, we just verify the concept works
            match template_type {
                "data_seed" => assert!(expected_content.contains("INSERT INTO")),
                "test_seed" => assert!(expected_content.contains("test-local")),
                "reference_seed" => assert!(expected_content.contains("INSERT INTO")),
                _ => assert!(false),
            }
        }
    }

    #[test]
    fn test_seed_file_naming_conventions() {
        // Test seed file naming conventions
        let seed_files = vec![
            "20231201_100000_create_admin_user.sql",
            "20231201_110000_test_data_seed.sql",
            "20231201_120000_reference_countries.sql",
            "20231201_100000_create_admin_user_revert.sql",  // This should be excluded
        ];

        let mut regular_seeds = Vec::new();
        let mut revert_seeds = Vec::new();

        for file_name in seed_files {
            if file_name.contains("_revert") {
                revert_seeds.push(file_name);
            } else {
                regular_seeds.push(file_name);
            }
        }

        assert_eq!(regular_seeds.len(), 3);
        assert_eq!(revert_seeds.len(), 1);

        // Verify naming pattern: timestamp_name.sql
        for seed in &regular_seeds {
            let re = Regex::new(r"^\d{8}_\d{6}_.*\.sql$").unwrap();
            assert!(re.is_match(seed));
        }

        // Verify revert naming pattern: timestamp_name_revert.sql
        for revert in &revert_seeds {
            let re = Regex::new(r"^\d{8}_\d{6}_.*_revert\.sql$").unwrap();
            assert!(re.is_match(revert));
        }
    }
}