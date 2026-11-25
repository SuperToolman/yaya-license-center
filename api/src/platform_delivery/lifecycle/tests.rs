use super::*;
#[test]
fn deployment_specs_reject_duplicate_mounts_and_bad_paths() {
    let valid = DeploymentSpec {
        environment: vec![EnvironmentSpec {
            name: "TOKEN".into(),
            required: true,
            default: None,
        }],
        volumes: vec![VolumeSpec {
            key: "data".into(),
            destination: "/data".into(),
        }],
        web_port: 8787,
        api_port: 8788,
    };
    assert!(valid.validate().is_ok());
    let mut v = valid.clone();
    v.volumes.push(v.volumes[0].clone());
    assert!(v.validate().is_err());
    let mut v = valid.clone();
    v.volumes[0].destination = "/data/../etc".into();
    assert!(v.validate().is_err());
    let mut v = valid.clone();
    v.environment[0].name = "A=B".into();
    assert!(v.validate().is_err());
}
#[test]
fn target_environment_removes_retired_keys_and_requires_new_values() {
    let spec = DeploymentSpec {
        environment: vec![
            EnvironmentSpec {
                name: "NEW".into(),
                required: true,
                default: None,
            },
            EnvironmentSpec {
                name: "KEEP".into(),
                required: false,
                default: Some("target-default".into()),
            },
        ],
        volumes: vec![],
        web_port: 8787,
        api_port: 8788,
    };
    let mut req = Request {
        task_id: Uuid::new_v4().to_string(),
        kind: "upgrade".into(),
        expected_container_id: "a".repeat(64),
        package_id: None,
        recovery_point_id: None,
        environment: BTreeMap::new(),
        remove_environment: vec![],
        acknowledge_data_restore: false,
    };
    let old = BTreeMap::from([
        ("KEEP".into(), "customer-override".into()),
        ("RETIRED".into(), "old-value".into()),
    ]);
    assert!(resolve_environment(&old, &spec, &req).is_err());
    req.environment.insert("NEW".into(), "new-value".into());
    let values = resolve_environment(&old, &spec, &req).unwrap();
    assert_eq!(values["KEEP"], "customer-override");
    assert_eq!(values["NEW"], "new-value");
    assert!(!values.contains_key("RETIRED"));
    req.remove_environment.push("NEW".into());
    assert!(resolve_environment(&old, &spec, &req).is_err());
}

#[tokio::test]
async fn version_history_is_admin_only_and_lifecycle_lock_survives_restart() {
    for role in ["platform_admin", "platform_operator"] {
        let f = super::super::tests::Fixture::new(role);
        assert_eq!(
            f.call("GET", "/api/platform-instances/x/versions", None, false)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        if role == "platform_operator" {
            assert_eq!(
                f.call("GET", "/api/platform-instances/x/versions", None, true)
                    .await
                    .0,
                StatusCode::FORBIDDEN
            );
            continue;
        }
        let d = db(&f.state).unwrap();
        d.execute("INSERT INTO platform_instances(instance_id,name,customer_id,package_id,web_port,api_port,created_by,created_at,updated_at,container_name) VALUES('i','test','c1','p1',18001,18002,'u',1,1,'test-lock')",[]).unwrap();
        assert!(ensure_idle(&d, "i").is_ok());
        d.execute("INSERT INTO container_version_tasks(id,instance_id,kind,status,stage,source_container_id,request_encrypted,actor_id,created_at) VALUES('t','i','backup','running','backing_up','container','sealed','u',1)",[]).unwrap();
        assert!(ensure_idle(&d, "i").is_err());
        recover(&f.state.database_path).unwrap();
        assert_eq!(
            d.query_row(
                "SELECT status FROM container_version_tasks WHERE id='t'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "attention"
        );
        assert!(ensure_idle(&d, "i").is_err());
        let (status, response) = f
            .call("GET", "/api/platform-instances/i/versions", None, true)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert!(!response.to_string().contains("sealed"));
    }
}
#[test]
fn snapshots_are_encrypted_bound_to_revision_and_tampering_is_rejected() {
    let f = super::super::tests::Fixture::new("platform_admin");
    let secret = json!({"environment":{"PASSWORD":"do-not-expose"}});
    let encrypted = seal(&f.state, "r1", &secret).unwrap();
    assert!(!encrypted.contains("do-not-expose"));
    assert_eq!(unseal(&f.state, "r1", &encrypted).unwrap(), secret);
    assert!(unseal(&f.state, "r2", &encrypted).is_err());
}
