pipeline {
    agent any
    triggers {
        GenericTrigger(
            genericVariables: [
                [key: 'ref', value: '$.ref'],
                [key: 'pr', value: '$.pull_request'],
            ],
            genericHeaderVariables: [
                [key: 'X-GitHub-Event', regexpFilter: ''],
            ],
            token: 'my-project-token',
        )
    }
    stages {
        stage('Build') {
            steps {
                sh "git checkout ${ref}"
                sh "echo ${pr_title} ${x_github_event}"
            }
        }
    }
}
