# Terraform

Creates the AWS infrastructure needed to run ckb bench.

These are the instructions for deploying to AWS.

## Requirements

- An AWS account
- Terraform (tested with 0.12)

## Deployment

- Create `terraform.tfvars` file and setup variables

```
access_key     = "XXXXXXXXXXXXXXXXXXXX"
secret_key     = "XXXXXXXXXXXXXXXXXXXX"
instance_count = 2
ssh_key        = "ssh-rsa ..."
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
terraform output ansible_hosts > ../../ansible/hosts
```
