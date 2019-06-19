provider "aws" {
    region     = "${var.region}"
    access_key = "${var.access_key}"
    secret_key = "${var.secret_key}"
}

resource "aws_vpc" "vpc" {
    cidr_block           = "10.0.0.0/16"
    enable_dns_hostnames = true
    tags = {
        Name = "${var.prefix}-vpc"
    }
}

resource "aws_internet_gateway" "ig" {
    vpc_id = "${aws_vpc.vpc.id}"
    tags = {
        Name = "${var.prefix}-ig"
    }
}

# Grant the VPC internet access on its main route table
resource "aws_route" "internet_access" {
    route_table_id         = "${aws_vpc.vpc.main_route_table_id}"
    destination_cidr_block = "0.0.0.0/0"
    gateway_id             = "${aws_internet_gateway.ig.id}"
}

resource "aws_subnet" "subnet" {
    vpc_id                  = "${aws_vpc.vpc.id}"
    cidr_block              = "10.0.1.0/24"
    map_public_ip_on_launch = true
    tags = {
        Name = "${var.prefix}-subnet"
    }
}

resource "aws_security_group" "sg" {
    name        = "${var.prefix}-default-sg"
    description = "Allow inbound ssh access, inbound all vpc access and outbound all access"
    vpc_id      = "${aws_vpc.vpc.id}"

    ingress {
        from_port   = 22
        to_port     = 22
        protocol    = "tcp"
        cidr_blocks = ["0.0.0.0/0"]
    }

    # inbound access from the VPC
    ingress {
        from_port   = 0
        to_port     = 0
        protocol    = "-1"
        cidr_blocks = ["10.0.0.0/16"]
    }

    # outbound access to all
    egress {
        from_port   = 0
        to_port     = 0
        protocol    = "-1"
        cidr_blocks = ["0.0.0.0/0"]
    }
}
resource "aws_key_pair" "ssh" {
    key_name = "ssh_key"
    public_key = "${file(var.public_key_path)}"
}

resource "aws_instance" "instance" {
    count                  = "${var.instance_count}"
    key_name               = "${aws_key_pair.ssh.id}"
    instance_type          = "${var.instance_type}"
    ami                    = "${data.aws_ami.ubuntu.id}"
    vpc_security_group_ids = ["${aws_security_group.sg.id}"]
    subnet_id              = "${aws_subnet.subnet.id}"

    tags = {
        Name = "${var.prefix}-instance-${count.index}"
    }
}

resource "null_resource" "instance_provisioners" {
    count           = "${var.instance_count}"
    # Changes to any instance of the instances requires re-provisioning
    triggers = {
        cluster_instance_ids = "${join(",", aws_instance.instance.*.id)}"
    }

    connection {
        host        = "${element(aws_instance.instance.*.public_ip, count.index)}"
        # The default username for our AMI
        user        = "ubuntu"
        private_key = "${file(var.private_key_path)}"
    }

    # Install python for ansible
    provisioner "remote-exec" {
        inline = [
        "sudo apt -y update",
        "sudo apt -y install python-minimal",
        ]
    }
}

output "public_ips" {
    value = "${join("\n", aws_instance.instance.*.public_ip)}"
}
