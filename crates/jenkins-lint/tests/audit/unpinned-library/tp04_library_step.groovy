library 'shared@feature/new-deploy'

def utils = library('utils@main').com.example.Utils

pipeline {
    agent { label 'linux' }
    stages {
        stage('Build') {
            steps {
                script {
                    library identifier: 'deploy@develop', retriever: modernSCM(
                        [$class: 'GitSCMSource', remote: 'https://example.com/deploy.git'])
                }
            }
        }
    }
}
