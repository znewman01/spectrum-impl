variable "ami" {
  type = string
}

variable "region" {
  type    = string
  default = "us-east-2"
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
  rsa_bits  = 4096
}

resource "aws_key_pair" "key" {
  public_key = tls_private_key.key.public_key_openssh
  tags = {
    Project = "spectrum",
    Name    = "spectrum_riposte_keypair"
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
    Project = "spectrum",
    Name    = "spectrum_riposte_security_group"
  }
}

resource "aws_instance" "leader" {
  ami             = var.ami
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
  tags = {
    Project = "spectrum",
    Name    = "spectrum_riposte_leader"
  }
}

resource "aws_instance" "server" {
  ami             = var.ami
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
  tags = {
    Project = "spectrum",
    Name    = "spectrum_riposte_server"
  }
}

resource "aws_instance" "auditor" {
  ami             = var.ami
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
  tags = {
    Project = "spectrum",
    Name    = "spectrum_riposte_auditor"
  }
}

resource "aws_instance" "client" {
  ami             = var.ami
  count           = 2 # TODO(zjn): make it 8 # from Riposte paper
  instance_type   = var.instance_type
  key_name        = aws_key_pair.key.key_name
  security_groups = [aws_security_group.allow_ssh.name]
  tags = {
    Project = "spectrum",
    Name    = "spectrum_riposte_client"
  }
}

output "leader" {
  value = aws_instance.leader.public_dns
}

output "server" {
  value = aws_instance.server.public_dns
}

output "auditor" {
  value = aws_instance.auditor.public_dns
}

output "clients" {
  value = "${aws_instance.client.*.public_dns}"
}

output "private_key" {
  value = tls_private_key.key.private_key_pem
}
