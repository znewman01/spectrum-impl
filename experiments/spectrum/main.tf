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
  alias  = "east"
  region = "us-east-1"
  default_tags { tags = local.tags }
}
provider "aws" {
  alias  = "west"
  region = "us-west-1"
  default_tags { tags = local.tags }
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

resource "tls_private_key" "main" {
  algorithm = "RSA"
  rsa_bits  = 4096
}

module "image_main" {
  source        = "../modules/image"
  image_name    = "spectrum_image"
  instance_type = var.instance_type
  providers     = { aws = aws }
  extra_filters = var.sha != "null" ? { "tag:Sha" = [var.sha] } : {}
}
module "image_east" {
  source        = "../modules/image"
  image_name    = "spectrum_image"
  instance_type = var.instance_type
  providers     = { aws = aws.east }
  extra_filters = var.sha != "null" ? { "tag:Sha" = [var.sha] } : {}
}
module "image_west" {
  source        = "../modules/image"
  image_name    = "spectrum_image"
  instance_type = var.instance_type
  providers     = { aws = aws.west }
  extra_filters = var.sha != "null" ? { "tag:Sha" = [var.sha] } : {}
}

module "network_main" {
  source     = "../modules/net"
  public_key = tls_private_key.main.public_key_openssh
  providers  = { aws = aws }
}
module "network_east" {
  source     = "../modules/net"
  public_key = tls_private_key.main.public_key_openssh
  providers  = { aws = aws.east }
}
module "network_west" {
  source     = "../modules/net"
  public_key = tls_private_key.main.public_key_openssh
  providers  = { aws = aws.west }
}

resource "aws_instance" "publisher" {
  ami             = module.image_main.ami.id
  instance_type   = var.instance_type
  key_name        = module.network_main.key_pair.key_name
  security_groups = [module.network_main.security_group.name]
  tags = {
    Name = "spectrum_publisher"
  }
}

resource "aws_instance" "worker" {
  provider        = aws.east # TODO: east AND west
  ami             = module.image_east.ami.id
  count           = var.worker_machine_count
  instance_type   = var.instance_type
  key_name        = module.network_east.key_pair.key_name
  security_groups = [module.network_east.security_group.name]
  tags            = { Name = "spectrum_worker" }
}

resource "aws_instance" "client" {
  ami             = module.image_main.ami.id
  count           = var.client_machine_count
  instance_type   = var.instance_type
  key_name        = module.network_main.key_pair.key_name
  security_groups = [module.network_main.security_group.name]
  tags            = { Name = "spectrum_client" }
}

locals {
  instances = concat(aws_instance.client, aws_instance.worker, [aws_instance.publisher])
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
  value     = tls_private_key.main.private_key_pem
  sensitive = true
}
