provider "azurerm" {
    version = "~> 1.27"
    subscription_id = "${var.subscription_id}"
    tenant_id = "${var.tenant_id}"
    client_id = "${var.client_id}"
    client_secret = "${var.client_secret}"
}

resource "azurerm_resource_group" "rg" {
    name     = "${var.prefix}-rg"
    location = "${var.location}"
}

resource "azurerm_virtual_network" "vnet" {
    name                = "${var.prefix}-vnet"
    address_space       = ["10.0.0.0/16"]
    location            = "${var.location}"
    resource_group_name = "${azurerm_resource_group.rg.name}"
}

resource "azurerm_subnet" "subnet" {
    name                 = "${var.prefix}-subnet"
    resource_group_name  = "${azurerm_resource_group.rg.name}"
    virtual_network_name = "${azurerm_virtual_network.vnet.name}"
    address_prefix       = "10.0.1.0/24"
}

resource "azurerm_public_ip" "publicip" {
    count                        = "${var.vm_count}"
    name                         = "${var.prefix}-publicip-${count.index}"
    location                     = "${var.location}"
    resource_group_name          = "${azurerm_resource_group.rg.name}"
    allocation_method            = "Dynamic"
}

resource "azurerm_network_security_group" "nsg" {
    name                = "${var.prefix}-nsg"
    location            = "${var.location}"
    resource_group_name = "${azurerm_resource_group.rg.name}"

    security_rule {
        name                       = "SSH"
        priority                   = 1001
        direction                  = "Inbound"
        access                     = "Allow"
        protocol                   = "Tcp"
        source_port_range          = "*"
        destination_port_range     = "22"
        source_address_prefix      = "*"
        destination_address_prefix = "*"
    }
}

resource "azurerm_network_interface" "nic" {
    count                     = "${var.vm_count}"
    name                      = "${var.prefix}-nic-${count.index}"
    location                  = "${var.location}"
    resource_group_name       = "${azurerm_resource_group.rg.name}"
    network_security_group_id = "${azurerm_network_security_group.nsg.id}"

    ip_configuration {
        name                          = "${var.prefix}-ipc-${count.index}"
        subnet_id                     = "${azurerm_subnet.subnet.id}"
        private_ip_address_allocation = "dynamic"
        public_ip_address_id          = "${element(azurerm_public_ip.publicip.*.id, count.index)}"
    }
}

resource "azurerm_virtual_machine" "vm" {
    count                 = "${var.vm_count}"
    name                  = "${var.prefix}-vm-${count.index}"
    location              = "${var.location}"
    resource_group_name   = "${azurerm_resource_group.rg.name}"
    network_interface_ids = ["${element(azurerm_network_interface.nic.*.id, count.index)}"]
    vm_size               = "${var.vm_size}"

    storage_os_disk {
        name              = "${var.prefix}-os-disk-${count.index}"
        caching           = "ReadWrite"
        create_option     = "FromImage"
        managed_disk_type = "Premium_LRS"
    }

    storage_image_reference {
        publisher = "Canonical"
        offer     = "UbuntuServer"
        sku       = "18.04-LTS"
        version   = "latest"
    }

    os_profile {
        computer_name  = "${var.prefix}-node-${count.index}"
        admin_username = "ubuntu"
    }

    os_profile_linux_config {
        disable_password_authentication = true

        ssh_keys {
            path     = "/home/ubuntu/.ssh/authorized_keys"
            key_data = "${var.ssh_key}"
        }
    }
}

output "public_ips" {
  value = "${join("\n", azurerm_public_ip.publicip.*.ip_address)}"
}
