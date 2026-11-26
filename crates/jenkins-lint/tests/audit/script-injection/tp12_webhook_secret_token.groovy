properties([
    pipelineTriggers([
        [$class: 'GenericTrigger',
         genericVariables: [
             [key: 'title', value: '$.pull_request.title'],
             [key: 'sha', value: '$.after'],
             [key: 'clean_ref', value: '$.ref', regexpFilter: '[^A-Za-z0-9._/-]'],
         ],
         tokenCredentialId: 'webhook-token',
        ]
    ])
])

node {
    sh "echo ${title}"
    sh "git checkout ${sha}"
    sh "git checkout ${clean_ref}"
}
