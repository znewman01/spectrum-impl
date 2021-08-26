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

variable "sha" {
  type = string
}

variable "region" {
  type = string
}

variable "instance_type" {
  type = string
}

variable "client_machine_count" {
  type = number
}

variable "worker_machine_count" {
  type = number
}

resource "tls_private_key" "key" {
  algorithm = "RSA"
  rsa_bits  = 4096
}

resource "aws_key_pair" "key" {
  public_key = tls_private_key.key.public_key_openssh
  tags = {
    Name = "spectrum_keypair"
  }
}

data "aws_ami" "spectrum" {
  count       = var.ami == "" ? 1 : 0
  most_recent = true
  owners      = ["self"]
  filter {
    name   = "tag:Project"
    values = ["spectrum"]
  }
  filter {
    name   = "tag:Name"
    values = ["spectrum_image"]
  }
  filter {
    name   = "tag:InstanceType"
    values = [var.instance_type]
  }
  filter {
    name   = "tag:Sha"
    values = [var.sha]
  }
}

locals {
  ami = var.ami != "" ? var.ami : data.aws_ami.spectrum[0].id
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
    Name = "spectrum_security_group"
  }
}


resource "aws_instance" "publisher" {
  ami             = local.ami
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
  tags = {
    Name = "spectrum_publisher"
  }
}

resource "aws_instance" "worker" {
  ami             = local.ami
  count           = var.worker_machine_count
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
  tags = {
    Name = "spectrum_worker"
  }
}

resource "aws_instance" "client" {
  ami             = local.ami
  count           = var.client_machine_count
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
  tags = {
    Name = "spectrum_client"
  }
}

output "publisher" {
  value = aws_instance.publisher.public_dns
}

output "workers" {
  value = aws_instance.worker.*.public_dns
}

output "clients" {
  value = aws_instance.client.*.public_dns
}

output "private_key" {
  value     = tls_private_key.key.private_key_pem
  sensitive = true
}
