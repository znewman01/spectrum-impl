terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 3.0"
    }
  }
}

variable "security_group" {
  type = object({ id = string })
}

variable "instances" {
  type = list(object({ public_ip = string }))
}

resource "aws_security_group_rule" "ping" {
  type              = "ingress"
  description       = "Spectrum traffic internal: leader+publisher"
  from_port         = -1
  to_port           = -1
  cidr_blocks       = ["0.0.0.0/0"]
  protocol          = "icmp"
  security_group_id = var.security_group.id
}

# Should be 2379, 6000-60001 and 6100-6110, but we're bumping up against quotas.
resource "aws_security_group_rule" "etcd_and_leader_and_publisher_and_workers" {
  type              = "ingress"
  description       = "Spectrum traffic internal: leader+publisher"
  from_port         = 2379
  to_port           = 6110
  cidr_blocks       = [for i in var.instances : "${i.public_ip}/32" if i.public_ip != ""]
  protocol          = "tcp"
  security_group_id = var.security_group.id
}
