variable "ami" {
  type = string
}

variable "region" {
  type = string
}

variable "instance_type" {
  type = string
}

provider "aws" {
  profile = "default"
  region  = var.region
}

resource "tls_private_key" "key" {
  algorithm = "RSA"
  rsa_bits = 4096
}

resource "aws_key_pair" "key" {
  public_key = tls_private_key.key.public_key_openssh
}

resource "aws_security_group" "allow_ssh" {
  ingress {
    description = "SSH from internet"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  egress {
    description = "All outgoing traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_instance" "example" {
  ami             = var.ami
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
}

output "hostname" {
  value = aws_instance.example.public_dns
}

output "private_key" {
  value = tls_private_key.key.private_key_pem
}
