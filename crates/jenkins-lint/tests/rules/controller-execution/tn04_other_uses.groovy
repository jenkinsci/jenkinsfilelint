// The branch master and a label parameter of other steps are not agents.
pipeline {
    agent { label 'linux' }
    stages {
        stage('Deploy') {
            when { branch 'master' }
            steps {
                git branch: 'master', url: 'https://example.com/repo.git'
                build job: 'downstream', parameters: [
                    [$class: 'LabelParameterValue', name: 'node', label: 'master']
                ]
                echo 'built-in'
            }
        }
    }
}
