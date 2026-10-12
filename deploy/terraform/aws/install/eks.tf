module "eks" {
  source  = "terraform-aws-modules/eks/aws"
  version = "~> 21.0"

  name               = var.name
  kubernetes_version = var.eks_version

  vpc_id     = module.vpc.vpc_id
  subnet_ids = module.vpc.private_subnets

  endpoint_public_access       = true
  endpoint_public_access_cidrs = var.eks_public_access_cidrs

  # Whoever runs this gets cluster admin, so bootstrap can install Argo CD next, unless a
  # pipeline applies it: then the admins are listed, so access doesn't follow the last runner.
  enable_cluster_creator_admin_permissions = var.cluster_creator_admin
  access_entries = merge(
    { for name, arn in var.cluster_admins : name => {
      principal_arn = arn
      policy_associations = { admin = {
        policy_arn   = "arn:aws:eks::aws:cluster-access-policy/AmazonEKSClusterAdminPolicy"
        access_scope = { type = "cluster" }
      } }
    } },
    { for name, arn in var.cluster_viewers : name => {
      principal_arn = arn
      policy_associations = { view = {
        policy_arn   = "arn:aws:eks::aws:cluster-access-policy/AmazonEKSViewPolicy"
        access_scope = { type = "cluster" }
      } }
    } },
  )

  addons = {
    vpc-cni = {
      before_compute = true
      # Enforce Kubernetes NetworkPolicies (the awzy chart's default-deny ingress).
      configuration_values = jsonencode({ enableNetworkPolicy = "true" })
    }
    eks-pod-identity-agent = { before_compute = true }
    kube-proxy             = {}
    coredns                = {}
    aws-ebs-csi-driver = {
      pod_identity_association = [{
        role_arn        = aws_iam_role.ebs_csi.arn
        service_account = "ebs-csi-controller-sa"
      }]
    }
  }

  # Named pools from the cluster contract. small runs everything on "general"; the hot and
  # data pools come with the standard profile.
  eks_managed_node_groups = {
    general = {
      ami_type       = "AL2023_x86_64_STANDARD"
      instance_types = ["t3.large"]
      min_size       = 2
      max_size       = 3
      desired_size   = 2
      labels         = { "awzy.io/pool" = "general" }
    }
  }
}

resource "aws_iam_role" "ebs_csi" {
  name               = "${var.name}-ebs-csi"
  assume_role_policy = data.aws_iam_policy_document.pod_identity_trust.json
}

resource "aws_iam_role_policy_attachment" "ebs_csi" {
  role       = aws_iam_role.ebs_csi.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonEBSCSIDriverPolicy"
}
