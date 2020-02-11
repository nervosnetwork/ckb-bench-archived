# Ansible

Setup softwares needed to run ckb bench.

These are the instructions for setup.

## Requirements

- Linux instances created by terraform
- Ansible (tested with 2.8)

## Deployment


- In this directory, run ansible:

```
# setup bastion first
ansible-playbook -vv -i hosts provision.yml --limit bastion
# setup other instances
ansible-playbook -vv -i hosts provision.yml --fork 10
```

## TC netem

- Setup 100ms delay on all instances

```
ansible -i hosts -u ubuntu -a "sudo tc qdisc add dev eth0 root netem delay 100ms" -b instances
ansible -i hosts -u ubuntu -a "sudo tc qdisc del dev eth0 root" -b instances
```

## Perf

- Restart ckb service and generate flamegraph

```
# start with perf on instance 1
ansible -i hosts -u ubuntu -m script -a "roles/instances/files/ckb.service.perf.sh perf" -b instances --limit instances[1]

# start without perf on instance 1
ansible -i hosts -u ubuntu -m script -a "roles/instances/files/ckb.service.perf.sh" -b instances --limit instances[1]
```
