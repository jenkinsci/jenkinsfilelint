pipeline {
    agent any
    stages {
        stage('Deploy') {
            steps {
                sh "kubectl -n ${params.NAMESPACE} rollout restart deployment/app"
                sh "helm upgrade app ./chart --set image.tag=${params['TAG']}"
            }
        }
    }
}
