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

variable "region" {
  type    = string
  default = "us-east-2"
}

variable "instance_type" {
  type = string
}

locals {
  tags = { Project = "spectrum" }
}

provider "aws" {
  region = var.region
  default_tags {
    tags = local.tags
  }
}
provider "aws" {
  alias  = "east"
  region = "us-east-1"
  default_tags {
    tags = local.tags
  }
}
provider "aws" {
  alias  = "west"
  region = "us-west-1"
  default_tags {
    tags = local.tags
  }
}

module "image_main" {
  source        = "../modules/image"
  image_name    = "riposte_image"
  instance_type = var.instance_type
  providers     = { aws = aws }
}
module "image_east" {
  source        = "../modules/image"
  image_name    = "riposte_image"
  instance_type = var.instance_type
  providers     = { aws = aws.east }
}
module "image_west" {
  source        = "../modules/image"
  image_name    = "riposte_image"
  instance_type = var.instance_type
  providers     = { aws = aws.west }
}

resource "tls_private_key" "key" {
  algorithm = "RSA"
  rsa_bits  = 4096
}

resource "aws_key_pair" "main" {
  public_key = tls_private_key.key.public_key_openssh
  tags = {
    Name = "riposte_keypair"
  }
}

module "network_main" {
  source     = "../modules/net"
  public_key = tls_private_key.key.public_key_openssh
  providers  = { aws = aws }
}
module "network_east" {
  source     = "../modules/net"
  public_key = tls_private_key.key.public_key_openssh
  providers  = { aws = aws.east }
}
module "network_west" {
  source     = "../modules/net"
  public_key = tls_private_key.key.public_key_openssh
  providers  = { aws = aws.west }
}

resource "aws_instance" "leader" {
  provider        = aws.east
  ami             = module.image_east.ami.id
  instance_type   = var.instance_type
  key_name        = module.network_east.key_pair.key_name
  security_groups = [module.network_east.security_group.name]
  tags = {
    Name = "riposte_leader"
  }
}

resource "aws_instance" "server" {
  provider        = aws.west
  ami             = module.image_west.ami.id
  instance_type   = var.instance_type
  key_name        = module.network_west.key_pair.key_name
  security_groups = [module.network_west.security_group.name]
  tags = {
    Name = "riposte_server"
  }
}

resource "aws_instance" "auditor" {
  ami             = module.image_main.ami.id
  instance_type   = var.instance_type
  key_name        = module.network_main.key_pair.key_name
  security_groups = [module.network_main.security_group.name]
  tags = {
    Name = "riposte_auditor"
  }
}

resource "aws_instance" "client" {
  ami             = module.image_main.ami.id
  count           = 8
  instance_type   = var.instance_type
  key_name        = module.network_main.key_pair.key_name
  security_groups = [module.network_main.security_group.name]
  tags = {
    Name = "riposte_client"
  }
}

locals {
  instances = concat(aws_instance.client, [aws_instance.auditor, aws_instance.server, aws_instance.leader])
}

module "secgroup_main" {
  source         = "./modules/secgroup"
  instances      = local.instances
  security_group = module.network_main.security_group
  providers      = { aws = aws }
}
module "secgroup_east" {
  source         = "./modules/secgroup"
  instances      = local.instances
  security_group = module.network_east.security_group
  providers      = { aws = aws.east }
}
module "secgroup_west" {
  source         = "./modules/secgroup"
  instances      = local.instances
  security_group = module.network_west.security_group
  providers      = { aws = aws.west }
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
  value = aws_instance.client.*.public_dns
}

output "private_key" {
  value     = tls_private_key.key.private_key_pem
  sensitive = true
}
