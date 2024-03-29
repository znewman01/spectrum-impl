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

resource "aws_security_group_rule" "main" {
  type              = "ingress"
  description       = "Express traffic internal."
  from_port         = 4442
  to_port           = 4443
  cidr_blocks       = [for i in var.instances : "${i.public_ip}/32"]
  protocol          = "tcp"
  security_group_id = var.security_group.id
}
