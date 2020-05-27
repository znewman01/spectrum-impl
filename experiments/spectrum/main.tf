variable "ami" {
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

provider "aws" {
  profile = "default"
  region  = var.region
  version = "~> 2.63"
}

provider "tls" {
  version = "~> 2.1"
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
}

resource "aws_instance" "publisher" {
  ami             = var.ami
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
}

resource "aws_instance" "worker" {
  ami             = var.ami
  count           = var.worker_machine_count
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
}

resource "aws_instance" "client" {
  ami             = var.ami
  count           = var.client_machine_count
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
}

output "publisher" {
  value = aws_instance.publisher.public_dns
}

output "workers" {
  value = "${aws_instance.worker.*.public_dns}"
}

output "clients" {
  value = "${aws_instance.client.*.public_dns}"
}

output "private_key" {
  value = tls_private_key.key.private_key_pem
}