# Ansible

Setup softwares needed to run ckb bench. This creates:

- CKB

These are the instructions for setup.

## Requirements

- Linux instances created by terraform
- Ansible (tested with 2.8)

## Deployment


- In this directory, run ansible:

```
ansible-playbook -i hosts provision.yml
```
