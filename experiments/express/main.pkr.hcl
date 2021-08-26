
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

variable "region" {
  type    = string
  default = "us-east-2"
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

source "amazon-ebs" "express" {
  access_key    = var.aws_access_key
  ami_name      = "express-${local.timestamp}"
  instance_type = var.instance_type
  region        = var.region
  secret_key    = var.aws_secret_key
  source_ami    = data.amazon-ami.ubuntu.id
  ssh_username  = "ubuntu"
  tags = {
    Name = "express_image"
    Project = "spectrum"
  }
}

build {
  sources = ["source.amazon-ebs.express"]

  provisioner "shell" {
    inline = ["while [ ! -f /var/lib/cloud/instance/boot-finished ]; do echo 'Waiting for cloud-init...'; sleep 1; done"]
  }

  provisioner "shell" {
    script = "./install.sh"
  }

  post-processor "manifest" {
    custom_data = {
      instance_type = var.instance_type
    }
    output = "manifest.json"
  }
}
