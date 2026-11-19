properties([
    parameters([
        string(name: 'BRANCH', defaultValue: 'main'),
        [$class: 'ChoiceParameterDefinition', name: 'REGION', choices: 'eu\nus'],
    ])
])

node {
    sh "git checkout ${params.BRANCH} && ./deploy.sh ${params.REGION}"
}
