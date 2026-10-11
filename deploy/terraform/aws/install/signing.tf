# The key awzy-policy signs policy bundles, enforcement mode changes, and the outage fallback
# with (ADR 0092). The private key never leaves KMS: only awzy-policy's role may sign, and the
# PDPs and gateways verify against the public key, which bootstrap writes from install_contract.
resource "aws_kms_key" "policy_signing" {
  description              = "${var.name}: awzy-policy signs policy with this key"
  customer_master_key_spec = "ECC_NIST_P256"
  key_usage                = "SIGN_VERIFY"
  deletion_window_in_days  = 30
}

resource "aws_kms_alias" "policy_signing" {
  name          = "alias/${var.name}-policy-signing"
  target_key_id = aws_kms_key.policy_signing.key_id
}

data "aws_kms_public_key" "policy_signing" {
  key_id = aws_kms_key.policy_signing.arn
}

data "aws_iam_policy_document" "policy" {
  statement {
    actions   = ["kms:Sign", "kms:GetPublicKey"]
    resources = [aws_kms_key.policy_signing.arn]
  }
}

resource "aws_iam_role_policy" "policy" {
  role   = aws_iam_role.workload["policy"].id
  policy = data.aws_iam_policy_document.policy.json
}
