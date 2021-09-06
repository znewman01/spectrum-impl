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

resource "aws_security_group_rule" "main" {
  type              = "ingress"
  description       = "Dissent traffic internal."
  from_port         = 0
  to_port           = 0
  cidr_blocks       = [for i in var.instances : "${i.public_ip}/32"]
  protocol          = "-1"
  security_group_id = var.security_group.id
}
