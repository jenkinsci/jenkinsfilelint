pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([[$class: 'AmazonWebServicesCredentialsBinding', credentialsId: 'aws', accessKeyVariable: 'AK', secretKeyVariable: 'SK']]) {
                    sh "aws configure set aws_access_key_id ${AK}"
                    sh "aws configure set aws_secret_access_key ${SK}"
                }
            }
        }
    }
}
