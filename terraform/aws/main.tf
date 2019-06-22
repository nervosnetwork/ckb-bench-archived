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

resource "aws_subnet" "subnet" {
    vpc_id                  = "${aws_vpc.vpc.id}"
    cidr_block              = "10.0.1.0/24"
    map_public_ip_on_launch = true
    tags = {
        Name = "${var.prefix}-subnet"
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

resource "aws_key_pair" "ssh" {
    key_name = "ssh_key"
    public_key = "${file(var.public_key_path)}"
}

resource "aws_security_group" "bastion-sg" {
    name        = "${var.prefix}-bastion-sg"
    vpc_id      = "${aws_vpc.vpc.id}"

    ingress {
        from_port   = 22
        to_port     = 22
        protocol    = "tcp"
        cidr_blocks = ["0.0.0.0/0"]
    }

    ingress {
        from_port   = 19999
        to_port     = 19999
        protocol    = "tcp"
        cidr_blocks = ["0.0.0.0/0"]
    }

    ingress {
        from_port   = 0
        to_port     = 0
        protocol    = "-1"
        cidr_blocks = ["${aws_subnet.subnet.cidr_block}"]
    }

    egress {
        from_port   = 0
        to_port     = 0
        protocol    = "-1"
        cidr_blocks = ["0.0.0.0/0"]
    }
}

# Create a bastion instance, this instance will be used as workload generator also.
resource "aws_instance" "bastion" {
    key_name                    = "${aws_key_pair.ssh.id}"
    instance_type               = "${var.instance_type}"
    ami                         = "${data.aws_ami.ubuntu.id}"
    vpc_security_group_ids      = ["${aws_security_group.bastion-sg.id}"]
    private_ip                  = "10.0.1.100"
    subnet_id                   = "${aws_subnet.subnet.id}"

    root_block_device {
        volume_size = "60"
    }

    connection {
        host        = "${aws_instance.bastion.public_ip}"
        user        = "ubuntu"
        private_key = "${file(var.private_key_path)}"
    }

    provisioner "remote-exec" {
        inline = [
        "sudo apt -y update",
        "sudo apt -y install python-minimal",
        ]
    }

    tags = {
        Name = "${var.prefix}-bastion"
    }
}

resource "aws_security_group" "sg" {
    name        = "${var.prefix}-default-sg"
    description = "Allow inbound access from vpc and outbound access to all"
    vpc_id      = "${aws_vpc.vpc.id}"

    ingress {
        from_port   = 0
        to_port     = 0
        protocol    = "-1"
        cidr_blocks = ["${aws_vpc.vpc.cidr_block}"]
    }

    egress {
        from_port   = 0
        to_port     = 0
        protocol    = "-1"
        cidr_blocks = ["0.0.0.0/0"]
    }
}

# Create a bootnode instance with fix private ip
resource "aws_instance" "bootnode" {
    key_name               = "${aws_key_pair.ssh.id}"
    instance_type          = "${var.instance_type}"
    ami                    = "${data.aws_ami.ubuntu.id}"
    vpc_security_group_ids = ["${aws_security_group.sg.id}"]
    private_ip             = "10.0.1.101"
    subnet_id              = "${aws_subnet.subnet.id}"

    root_block_device {
        volume_size = "60"
    }

    connection {
        bastion_host = "${aws_instance.bastion.public_ip}"
        host         = "${aws_instance.bootnode.private_ip}"
        user         = "ubuntu"
        private_key  = "${file(var.private_key_path)}"
    }

    provisioner "remote-exec" {
        inline = [
        "sudo apt -y update",
        "sudo apt -y install python-minimal",
        ]
    }

    tags = {
        Name = "${var.prefix}-bootnode"
    }
}

# Create other normal instances
resource "aws_instance" "instance" {
    count                  = "${var.instance_count}"
    key_name               = "${aws_key_pair.ssh.id}"
    instance_type          = "${var.instance_type}"
    ami                    = "${data.aws_ami.ubuntu.id}"
    vpc_security_group_ids = ["${aws_security_group.sg.id}"]
    subnet_id              = "${aws_subnet.subnet.id}"

    root_block_device {
        volume_size = "60"
    }

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
        bastion_host = "${aws_instance.bastion.public_ip}"
        host         = "${element(aws_instance.instance.*.private_ip, count.index)}"
        user         = "ubuntu"
        private_key  = "${file(var.private_key_path)}"
    }

    # Install python for ansible
    provisioner "remote-exec" {
        inline = [
        "sudo apt -y update",
        "sudo apt -y install python-minimal",
        ]
    }
}

output "bastion_public_ip" {
    value = "${aws_instance.bastion.public_ip}"
}

output "instance_private_ips" {
    value = "${join("\n", aws_instance.instance.*.private_ip)}"
}

output "ansible_hosts" {
    value = <<ANSIBLE_HOSTS
[bastion]
bastion-0            ansible_host=${aws_instance.bastion.public_ip}

[instances]
bootnode-0           ansible_host=${aws_instance.bootnode.private_ip}
${join("\n", formatlist("%s ansible_host=%s", aws_instance.instance.*.tags.Name, aws_instance.instance.*.private_ip))}
ANSIBLE_HOSTS
}
