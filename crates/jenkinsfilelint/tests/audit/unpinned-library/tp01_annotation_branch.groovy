@Library('pipeline-library@main') _

pipeline {
    agent { label 'linux' }
    stages {
        stage('Build') {
            steps {
                buildPlugin()
            }
        }
    }
}
