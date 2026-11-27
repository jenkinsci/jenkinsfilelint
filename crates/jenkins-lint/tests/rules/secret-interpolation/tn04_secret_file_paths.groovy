pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([file(credentialsId: 'kube', variable: 'KUBECONFIG'), sshUserPrivateKey(credentialsId: 'ssh', keyFileVariable: 'KEY')]) {
                    sh "kubectl --kubeconfig ${KUBECONFIG} apply -f app.yaml"
                    sh "ssh -i ${KEY} deploy@host"
                }
            }
        }
    }
}
