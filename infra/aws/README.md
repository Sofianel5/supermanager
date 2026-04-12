# AWS backend infrastructure

This Terraform stack provisions the AWS-native backend for Supermanager:

- ECR for the server image
- ECS Fargate for the Axum service
- ALB with TLS termination and `/health` checks
- PostgreSQL on RDS
- Secrets Manager wiring for `DATABASE_URL`
- CloudWatch log group and basic alarms
- Optional GitHub Actions OIDC deploy role

## Required inputs

Set these before `terraform apply`:

- `acm_certificate_arn`
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
