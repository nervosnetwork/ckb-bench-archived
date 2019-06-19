# Terraform

Creates the Azure infrastructure needed to run ckb bench.

These are the instructions for deploying to Azure.

## Requirements

- An Azure account
- Terraform (tested with 0.12)

## Deployment

- Create `terraform.tfvars` file and [setup variables](https://docs.microsoft.com/en-us/azure/virtual-machines/linux/terraform-install-configure#set-up-terraform-access-to-azure)

```
subscription_id = "xxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
tenant_id       = "xxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
client_id       = "xxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
client_secret   = "xxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
vm_count        = 20
```

- In this directory, run terraform:

```
terraform init
terraform plan
```

- Review the changes. If everything looks `green`, apply:

```
terraform apply
```

- Generates hosts file for ansible

```
terraform output public_ips > ../../ansible/hosts
```
