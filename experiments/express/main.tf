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

locals {
  tags = { Project = "spectrum" }
}

provider "aws" {
  region = var.region
  default_tags { tags = local.tags }
}
provider "aws" {
  alias  = "west"
  region = "us-west-1"
  default_tags { tags = local.tags }
}
provider "aws" {
  alias  = "east"
  region = "us-east-1"
  default_tags { tags = local.tags }
}

variable "region" {
  type    = string
  default = "us-east-2"
}

variable "instance_type" {
  type = string
}

module "image_main" {
  source        = "../modules/image"
  image_name    = "express_image"
  instance_type = var.instance_type
  providers     = { aws = aws }
}
module "image_east" {
  source        = "../modules/image"
  image_name    = "express_image"
  instance_type = var.instance_type
  providers     = { aws = aws.east }
}
module "image_west" {
  source        = "../modules/image"
  image_name    = "express_image"
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
    Name = "express_keypair"
  }
}

module "network_main" {
  source     = "../modules/net"
  public_key = tls_private_key.key.public_key_openssh
  providers = {
    aws = aws
  }
}
module "network_east" {
  source     = "../modules/net"
  public_key = tls_private_key.key.public_key_openssh
  providers = {
    aws = aws.east
  }
}
module "network_west" {
  source     = "../modules/net"
  public_key = tls_private_key.key.public_key_openssh
  providers = {
    aws = aws.west
  }
}
resource "aws_instance" "serverA" {
  ami             = module.image_east.ami.id
  provider        = aws.east
  instance_type   = var.instance_type
  key_name        = module.network_east.key_pair.key_name
  security_groups = [module.network_east.security_group.name]
  tags            = { Name = "express_serverA" }
}

resource "aws_instance" "serverB" {
  ami             = module.image_west.ami.id
  provider        = aws.west
  instance_type   = var.instance_type
  key_name        = module.network_west.key_pair.key_name
  security_groups = [module.network_west.security_group.name]
  tags            = { Name = "express_serverB" }
}

# TODO(zjn): add more client servers?
# Express evaluation only uses one but we (and Riposte) use >1
resource "aws_instance" "client" {
  ami             = module.image_main.ami.id
  instance_type   = var.instance_type
  key_name        = module.network_main.key_pair.key_name
  security_groups = [module.network_main.security_group.name]
  tags            = { Name = "express_client" }
}

locals {
  instances = [aws_instance.client, aws_instance.serverA, aws_instance.serverB]
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
