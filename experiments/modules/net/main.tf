terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 3.0"
    }
  }
}

variable "public_key" {
  type = string
}

resource "aws_key_pair" "main" {
  public_key = var.public_key
  tags = {
    Name = "spectrum_keypair"
  }
}


resource "aws_security_group" "main" {

  tags = {
    Name = "spectrum_security_group"
  }
}

resource "aws_security_group_rule" "allow_ssh" {
  description       = "SSH from internet"
  type              = "ingress"
  from_port         = 22
  to_port           = 22
  protocol          = "tcp"
  cidr_blocks       = ["0.0.0.0/0"]
  security_group_id = aws_security_group.main.id
}

resource "aws_security_group_rule" "within_group" {
  description       = "All traffic within group"
  type              = "ingress"
  from_port         = 0
  to_port           = 0
  protocol          = "-1"
  self              = true
  security_group_id = aws_security_group.main.id
}

resource "aws_security_group_rule" "outgoing" {
  description       = "All outgoing traffic"
  type              = "egress"
  from_port         = 0
  to_port           = 0
  protocol          = "-1"
  cidr_blocks       = ["0.0.0.0/0"]
  security_group_id = aws_security_group.main.id
}

output "security_group" {
  value = aws_security_group.main
}

output "key_pair" {
  value = aws_key_pair.main
}
