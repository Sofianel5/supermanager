output "alb_dns_name" {
  description = "AWS-managed DNS name for the public ALB."
  value       = aws_lb.server.dns_name
}

output "api_url" {
  description = "Intended public API URL once DNS is pointed at the ALB."
  value       = local.api_url
}

output "aws_region" {
  description = "AWS region used for backend resources."
  value       = var.aws_region
}

output "ecr_repository_url" {
  description = "ECR repository URL used by the server image."
  value       = aws_ecr_repository.server.repository_url
}

output "ecr_repository_name" {
  description = "ECR repository name used by the deploy workflow."
  value       = aws_ecr_repository.server.name
}

output "ecs_cluster_name" {
  description = "ECS cluster name for deploy workflow configuration."
  value       = aws_ecs_cluster.this.name
}

output "ecs_service_name" {
  description = "ECS service name for deploy workflow configuration."
  value       = aws_ecs_service.server.name
}

output "database_url_secret_arn" {
  description = "Secrets Manager ARN containing DATABASE_URL."
  value       = aws_secretsmanager_secret.database_url.arn
}

output "github_actions_role_arn" {
  description = "IAM role ARN for GitHub Actions OIDC, if enabled."
  value       = try(aws_iam_role.github_actions_deploy[0].arn, null)
}
