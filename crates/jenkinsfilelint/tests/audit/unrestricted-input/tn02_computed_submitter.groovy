// A submitter that Groovy computes still restricts who may approve.
def approvers = env.APPROVERS ?: 'release-managers'
input message: 'Proceed?', submitter: approvers
input message: 'Proceed?', submitter: "${approvers},alice"
