terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 3.0"
    }

    tls = {
      source  = "hashicorp/tls"
      version = "~> 3.1"
    }
  }
}

provider "aws" {
  region = var.region
  default_tags {
    tags = {
      Project = "spectrum"
    }
  }
}

variable "ami" {
  type    = string
  default = ""
}

variable "region" {
  type    = string
  default = "us-east-2"
}

variable "instance_type" {
  type = string
}

data "aws_ami" "express" {
  count       = var.ami == "" ? 1 : 0
  most_recent = true
  owners      = ["self"]
  filter {
    name   = "tag:Project"
    values = ["spectrum"]
  }
  filter {
    name   = "tag:Name"
    values = ["express_image"]
  }
  filter {
    name   = "tag:InstanceType"
    values = [var.instance_type]
  }
}

locals {
  ami = var.ami != "" ? var.ami : data.aws_ami.express[0].id
}

resource "tls_private_key" "key" {
  algorithm = "RSA"
  rsa_bits  = 4096
}

resource "aws_key_pair" "key" {
  public_key = tls_private_key.key.public_key_openssh
  tags = {
    Name = "express_keypair"
  }
}

resource "aws_security_group" "allow_ssh" {

  ingress {
    description = "SSH from internet"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  ingress {
    description = "All traffic within group"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    self        = true
  }

  egress {
    description = "All outgoing traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "express_security_group"
  }
}

resource "aws_instance" "serverA" {
  ami             = local.ami
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
  tags = {
    Name = "express_serverA"
  }
}

resource "aws_instance" "serverB" {
  ami             = local.ami
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
  tags = {
    Name = "express_serverB"
  }
}

# TODO(zjn): add more client servers?
# Express evaluation only uses one but we (and Riposte) use >1
resource "aws_instance" "client" {
  ami             = local.ami
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
  tags = {
    Name = "express_client"
  }
}

output "serverA" {
  value = aws_instance.serverA.public_dns
}

output "serverB" {
  value = aws_instance.serverB.public_dns
}

output "client" {
  value = aws_instance.client.public_dns
}

output "private_key" {
  value     = tls_private_key.key.private_key_pem
  sensitive = true
}
