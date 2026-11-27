pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([sshUserPrivateKey(credentialsId: 'ssh', keyFileVariable: 'KEY', passphraseVariable: 'PHRASE', usernameVariable: 'SSH_USER')]) {
                    sh "ssh -i ${KEY} ${SSH_USER}@host"
                    sh "unlock ${PHRASE}"
                }
            }
        }
    }
}
