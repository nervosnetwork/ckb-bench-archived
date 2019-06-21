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

## TC netem

- Setup 100ms delay on all instances

```
ansible -i hosts -u ubuntu -a "sudo tc qdisc add dev eth0 root netem delay 100ms" -b instances
ansible -i hosts -u ubuntu -a "sudo tc qdisc del dev eth0 root" -b instances
```
