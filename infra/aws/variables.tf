variable "aws_region" {
  description = "AWS region for all backend resources."
  type        = string
  default     = "us-west-1"
}

variable "name" {
  description = "Base name used for AWS resources."
  type        = string
  default     = "supermanager"
}

variable "vpc_cidr" {
  description = "CIDR block for the VPC."
  type        = string
  default     = "10.42.0.0/16"
}

variable "api_domain" {
  description = "Public API hostname served by the ALB."
  type        = string
  default     = "api.supermanager.dev"
}

variable "public_app_url" {
  description = "Public frontend URL used for generated dashboard links."
  type        = string
  default     = "https://supermanager.dev"
}

variable "container_name" {
  description = "Container name inside the ECS task definition."
  type        = string
  default     = "coordination-server"
}

variable "container_port" {
  description = "Container port exposed by the server."
  type        = number
  default     = 8787
}

variable "ecs_cpu" {
  description = "Fargate CPU units for the server task."
  type        = number
  default     = 512
}

variable "ecs_memory" {
  description = "Fargate memory (MiB) for the server task."
  type        = number
  default     = 1024
}

variable "log_retention_days" {
  description = "CloudWatch log retention in days."
  type        = number
  default     = 30
}

variable "acm_certificate_arn" {
  description = "ACM certificate ARN for the public API domain."
  type        = string
}

variable "openai_api_key_secret_arn" {
  description = "Secrets Manager ARN containing the API key value injected as CODEX_API_KEY."
  type        = string
}

variable "better_auth_secret_arn" {
  description = "Secrets Manager ARN containing BETTER_AUTH_SECRET."
  type        = string
}

variable "google_client_id_secret_arn" {
  description = "Secrets Manager ARN containing GOOGLE_CLIENT_ID."
  type        = string
}

variable "google_client_secret_arn" {
  description = "Secrets Manager ARN containing GOOGLE_CLIENT_SECRET."
  type        = string
}

variable "github_client_id_secret_arn" {
  description = "Secrets Manager ARN containing GITHUB_CLIENT_ID."
  type        = string
}

variable "github_client_secret_arn" {
  description = "Secrets Manager ARN containing GITHUB_CLIENT_SECRET."
  type        = string
}

variable "db_name" {
  description = "PostgreSQL database name."
  type        = string
  default     = "supermanager"
}

variable "db_username" {
  description = "PostgreSQL username."
  type        = string
  default     = "supermanager"
}

variable "db_instance_class" {
  description = "RDS instance class."
  type        = string
  default     = "db.t4g.micro"
}

variable "db_engine_version" {
  description = "PostgreSQL engine version."
  type        = string
  default     = "18.3"
}

variable "db_allocated_storage" {
  description = "Initial RDS storage allocation in GiB."
  type        = number
  default     = 20
}

variable "db_max_allocated_storage" {
  description = "Maximum autoscaled RDS storage in GiB."
  type        = number
  default     = 100
}

variable "db_backup_retention_days" {
  description = "RDS automated backup retention in days."
  type        = number
  default     = 7
}

variable "db_deletion_protection" {
  description = "Enable RDS deletion protection."
  type        = bool
  default     = false
}

variable "db_skip_final_snapshot" {
  description = "Skip a final snapshot when destroying the RDS instance."
  type        = bool
  default     = true
}

variable "alarm_topic_arn" {
  description = "Optional SNS topic ARN for CloudWatch alarm notifications."
  type        = string
  default     = null
}

variable "github_oidc_provider_arn" {
  description = "Existing IAM OIDC provider ARN for GitHub Actions."
  type        = string
  default     = null
}

variable "github_org" {
  description = "GitHub org or user allowed to assume the deploy role."
  type        = string
  default     = "Sofianel5"
}

variable "github_repo" {
  description = "GitHub repository allowed to assume the deploy role."
  type        = string
  default     = "supermanager"
}

variable "github_branch" {
  description = "Git branch allowed to assume the deploy role."
  type        = string
  default     = "master"
}

variable "tags" {
  description = "Additional tags applied to all resources."
  type        = map(string)
  default     = {}
}
