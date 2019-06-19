variable "subscription_id" {}
variable "tenant_id"  {}
variable "client_id"  {}
variable "client_secret" {}

variable "location" {
    type = string
    default = "eastus"
}

variable "prefix" {
    type    = string
    default = "ckb-bench"
}

variable "vm_count" {
    type = number
}

variable "vm_size" {
    type    = string
    default = "Standard_DS2_v2"
}

variable "ssh_key" {
    type = string
}
