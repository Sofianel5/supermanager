# AWS backend infrastructure

This Terraform stack provisions the AWS-native backend for Supermanager:

- ECR for the server image
- ECS Fargate for the backend service
- ALB with TLS termination and `/health` checks
- PostgreSQL on RDS
- EFS for durable Codex and per-room working state
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

The deploy workflow assumes the ECS service already exists, runs only from `master`, pushes the backend image to ECR as `:latest`, and forces a new ECS deployment so the service pulls that tag.

The ECS task definition is managed in Terraform and mounts EFS at `/srv/supermanager`, with `SUPERMANAGER_DATA_DIR=/srv/supermanager` set in the container environment. The service is configured as a single writer during deploys with `desired_count = 1`, `deployment_minimum_healthy_percent = 0`, and `deployment_maximum_percent = 100`.

The task definition also injects these auth-related secrets from Secrets Manager:

- `BETTER_AUTH_SECRET`
- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`
- `GITHUB_CLIENT_ID`
- `GITHUB_CLIENT_SECRET`
