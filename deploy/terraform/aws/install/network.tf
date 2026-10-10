module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 6.0"

  name = var.name
  cidr = var.vpc_cidr
  azs  = var.azs

  private_subnets = [for i, _ in var.azs : cidrsubnet(var.vpc_cidr, 4, i)]
  public_subnets  = [for i, _ in var.azs : cidrsubnet(var.vpc_cidr, 8, 200 + i)]

  enable_nat_gateway = true
  single_nat_gateway = var.size_profile == "small"

  public_subnet_tags  = { "kubernetes.io/role/elb" = 1 }
  private_subnet_tags = { "kubernetes.io/role/internal-elb" = 1 }
}

# Postgres, Kafka, and Valkey accept connections only from the cluster's nodes.
resource "aws_security_group" "data" {
  name        = "${var.name}-data"
  description = "Managed data services, reachable from EKS nodes only"
  vpc_id      = module.vpc.vpc_id
}

resource "aws_vpc_security_group_ingress_rule" "data_from_nodes" {
  for_each = {
    postgres  = 5432
    kafka     = 9092
    kafka_tls = 9094
    valkey    = 6379
    valkey_ro = 6380
  }
  security_group_id            = aws_security_group.data.id
  referenced_security_group_id = module.eks.node_security_group_id
  ip_protocol                  = "tcp"
  from_port                    = each.value
  to_port                      = each.value
  description                  = each.key
}
