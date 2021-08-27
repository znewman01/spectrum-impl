terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 3.0"
    }
  }
}

variable "image_name" {
  type = string
}

variable "instance_type" {
  type = string
}

variable "extra_filters" {
  type    = map
  default = {}
}

data "aws_ami" "main" {
  most_recent = true
  owners      = ["self"]
  filter {
    name   = "tag:Project"
    values = ["spectrum"]
  }
  filter {
    name   = "tag:Name"
    values = [var.image_name]
  }
  filter {
    name   = "tag:InstanceType"
    values = [var.instance_type]
  }

  dynamic "filter" {
    for_each = var.extra_filters
    content {
      name   = filter.key
      values = filter.value
    }
  }
}

output "ami" {
  value = data.aws_ami.main
}
