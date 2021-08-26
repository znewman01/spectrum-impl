variable "aws_access_key" {
  type    = string
  default = env("AWS_ACCESS_KEY_ID")
}

variable "aws_secret_key" {
  type    = string
  default = env("AWS_SECRET_ACCESS_KEY")
}

variable "instance_type" {
  type = string
}

variable "profile" {
  type    = string
  default = "debug"
}

variable "region" {
  type    = string
  default = "us-east-2"
}

variable "sha" {
  type    = string
  default = ""
}

variable "src_archive" {
  type    = string
  default = ""
}

data "amazon-ami" "ubuntu" {
  access_key = var.aws_access_key
  filters = {
    name                = "ubuntu/images/*ubuntu-focal-20.04-amd64-server-*"
    root-device-type    = "ebs"
    virtualization-type = "hvm"
  }
  most_recent = true
  owners      = ["099720109477"]
  region      = var.region
  secret_key  = var.aws_secret_key
}

locals { timestamp = regex_replace(timestamp(), "[- TZ:]", "") }

source "amazon-ebs" "spectrum" {
  access_key    = var.aws_access_key
  ami_name      = "spectrum-${local.timestamp}"
  instance_type = var.instance_type
  region        = var.region
  secret_key    = var.aws_secret_key
  source_ami    = data.amazon-ami.ubuntu.id
  ssh_username  = "ubuntu"
  tags = {
    Name = "spectrum_image"
    Project = "spectrum"
  }
}

build {
  sources = ["source.amazon-ebs.spectrum"]

  provisioner "shell" {
    inline = ["while [ ! -f /var/lib/cloud/instance/boot-finished ]; do echo 'Waiting for cloud-init...'; sleep 1; done"]
  }

  provisioner "file" {
    destination = "/home/ubuntu"
    source      = "config"
  }

  provisioner "shell" {
    script = "./install.sh"
  }

  provisioner "file" {
    destination = "/home/ubuntu/spectrum-src.tar.gz"
    source      = var.src_archive
  }

  provisioner "shell" {
    environment_vars = ["AWS_ACCESS_KEY_ID=${var.aws_access_key}", "AWS_SECRET_ACCESS_KEY=${var.aws_secret_key}", "SRC_SHA=${var.sha}", "INSTANCE_TYPE=${var.instance_type}", "PROFILE=${var.profile}"]
    script           = "./compile.sh"
  }

  post-processor "manifest" {
    custom_data = {
      instance_type = var.instance_type
      profile       = var.profile
      sha           = var.sha
    }
    output = "manifest.json"
  }
}
