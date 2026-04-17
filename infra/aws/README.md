# AWS backend infrastructure

This Terraform stack provisions the AWS-native backend for Supermanager:

- ECR for the server image
- ECS Fargate API service
- ECS Fargate summary worker service
- ALB with TLS termination and `/health` checks
- PostgreSQL on RDS
- EFS for durable Codex and summary-worker state
- Secrets Manager wiring for `DATABASE_URL`, Better Auth secrets, and `CODEX_API_KEY`
- CloudWatch log group and basic alarms
- Optional GitHub Actions OIDC deploy role

## Required inputs

Set these before `terraform apply`:

- `acm_certificate_arn`
- `better_auth_secret_arn`
- `google_client_id_secret_arn`
- `google_client_secret_arn`
- `github_client_id_secret_arn`
- `github_client_secret_arn`
- `openai_api_key_secret_arn`

Optionally set:

- `github_oidc_provider_arn` to create the GitHub Actions deploy role
- `alarm_topic_arn` to attach SNS notifications to alarms

## Apply

```sh
cd infra/aws
terraform init
terraform plan \
  -var='acm_certificate_arn=arn:aws:acm:us-west-2:123456789012:certificate/...' \
  -var='better_auth_secret_arn=arn:aws:secretsmanager:us-west-2:123456789012:secret:supermanager/better-auth-secret' \
  -var='google_client_id_secret_arn=arn:aws:secretsmanager:us-west-2:123456789012:secret:supermanager/google-client-id' \
  -var='google_client_secret_arn=arn:aws:secretsmanager:us-west-2:123456789012:secret:supermanager/google-client-secret' \
  -var='github_client_id_secret_arn=arn:aws:secretsmanager:us-west-2:123456789012:secret:supermanager/github-client-id' \
  -var='github_client_secret_arn=arn:aws:secretsmanager:us-west-2:123456789012:secret:supermanager/github-client-secret' \
  -var='openai_api_key_secret_arn=arn:aws:secretsmanager:us-west-2:123456789012:secret:supermanager/openai-api-key'
terraform apply
```

## GitHub Actions variables

After apply, set these repository variables from the Terraform outputs:

- `AWS_REGION` from `aws_region`
- `AWS_DEPLOY_ROLE_ARN` from `github_actions_role_arn`
- `AWS_ECR_REPOSITORY` from `ecr_repository_name`
- `AWS_ECS_CLUSTER` from `ecs_cluster_name`
- `AWS_ECS_SERVICE` from `ecs_service_name`
- `AWS_ECS_SUMMARY_WORKER_SERVICE` from `ecs_summary_worker_service_name`

The deploy workflow assumes both ECS services already exist, runs only from `master`, pushes the backend image to ECR as `:latest`, rolls the API service first so it can apply migrations, then restarts the summary worker service.

The API task definition is managed in Terraform and no longer mounts EFS. The summary worker task definition runs the Rust `summary-agent` binary directly, mounts EFS at `/srv/supermanager`, and sets `SUPERMANAGER_DATA_DIR=/srv/supermanager` in the container environment. The API service now rolls with `desired_count = 1`, `deployment_minimum_healthy_percent = 100`, and `deployment_maximum_percent = 200`, while the summary worker replays room summaries from Postgres using `room_summaries.last_processed_seq`.

The task definition also injects these auth-related secrets from Secrets Manager:

- `BETTER_AUTH_SECRET`
- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`
- `GITHUB_CLIENT_ID`
- `GITHUB_CLIENT_SECRET`
